"""Pure extracted diagnostic logic with fake UDP; never imports/runs the LSP worker."""
import json
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / 'native/tests/fixtures/advanced-lsp.mjs'
BEGIN = '// Explicit test-host correlation only. This channel cannot prove delivery or exit.\n'
END = 'let buffer = Buffer.alloc(0), uri, settingsChecked=false, pendingTypeQuery;'
DRIVER = r"""
const vm=require('node:vm');
const input=JSON.parse(require('node:fs').readFileSync(0,'utf8'));
const result={created:0,unref:0,attempts:0,messages:[]};
let now=0,errorHandler;
const socket={on(name,handler){if(name!=='error')throw Error('unexpected event');errorHandler=handler;},unref(){result.unref++;},send(bytes,port,address,callback){result.attempts++;if(input.throwSend)throw Error('fake send failure');result.messages.push({bytes:bytes.length,port,address,value:JSON.parse(bytes.toString())});callback();}};
const context=vm.createContext({Buffer,Map,Number,Math,JSON,performance:{now:()=>now},
 process:{argv:['node','owned-fixture','',input.port,input.nonce],pid:7},
 dgram:{createSocket(kind){if(kind!=='udp4')throw Error('unexpected transport');result.created++;return socket;}}});
vm.runInContext(input.block,context,{timeout:1000});
for(const event of input.events??[]){
 now=event.timeMs??0;
 if(event.socketError){errorHandler();continue;}
 context.phase=event.phase;context.method=event.method;
 vm.runInContext('observe(phase,method)',context,{timeout:1000});
}
process.stdout.write(JSON.stringify(result));
"""


def diagnostic_block(source):
    if source.count(BEGIN) != 1 or source.count(END) != 1:
        raise ValueError('Missing unique owned observation block')
    block = source.split(BEGIN, 1)[1].split(END, 1)[0]
    if 'process.stdin' in block or 'function handle(' in block or 'function send(' in block:
        raise ValueError('Worker execution is outside the pure observation check')
    return block


@unittest.skipUnless(shutil.which('node'), 'Node is needed for pure observation logic')
class FixtureObservations(unittest.TestCase):
    def run_logic(self, port='12345', nonce='a' * 64, events=(), throw_send=False):
        payload = {'block': diagnostic_block(FIXTURE.read_text(encoding='utf-8')),
                   'port': port, 'nonce': nonce, 'events': events, 'throwSend': throw_send}
        output = subprocess.check_output([shutil.which('node'), '--eval', DRIVER],
                                         input=json.dumps(payload).encode(), timeout=8)
        self.assertLessEqual(len(output), 64000)
        return json.loads(output)

    def test_no_socket_without_both_explicit_valid_host_selections(self):
        for port, nonce in [(None, 'a' * 64), ('0', 'a' * 64), ('65536', 'a' * 64),
                            ('other', 'a' * 64), ('12345', None), ('12345', 'a' * 65),
                            ('12345', '🌍' * 64)]:
            with self.subTest(port=port, nonce=nonce):
                result = self.run_logic(port, nonce)
                self.assertEqual(result['created'], 0)
                self.assertEqual(result['attempts'], 0)

    def test_labels_are_closed_and_no_source_params_or_arbitrary_method_is_retained(self):
        secret = 'untrusted provider text 🌍\n/path/secret'
        result = self.run_logic(events=[
            {'phase': 'received', 'method': 'textDocument/hover', 'timeMs': 23},
            {'phase': 'reply_queued', 'method': secret, 'timeMs': 24},
            {'phase': secret, 'method': 'initialize', 'timeMs': 25}])
        self.assertEqual(result['created'], 1)
        self.assertEqual(result['unref'], 1)
        self.assertEqual([m['value']['method'] for m in result['messages']],
                         ['startup', 'hover', 'other'])
        for message in result['messages']:
            self.assertEqual(message['address'], '127.0.0.1')
            self.assertEqual(message['port'], 12345)
            self.assertEqual(set(message['value']), {'nonce', 'worker', 'phase', 'method', 'timeMs'})
        self.assertNotIn(secret, json.dumps(result))

    def test_packet_count_and_cumulative_bytes_are_bounded(self):
        result = self.run_logic(events=[{'phase': 'received', 'method': 'textDocument/documentSymbol'}] * 1000)
        self.assertLessEqual(result['attempts'], 64)
        self.assertGreater(result['attempts'], 1)
        self.assertLessEqual(sum(m['bytes'] for m in result['messages']), 8192)
        self.assertTrue(all(m['bytes'] < 256 for m in result['messages']))

    def test_send_failure_and_socket_error_disable_diagnostics_without_throwing(self):
        result = self.run_logic(throw_send=True, events=[{'phase': 'received', 'method': 'initialize'}])
        self.assertEqual(result['attempts'], 1)
        self.assertEqual(result['messages'], [])
        result = self.run_logic(events=[{'socketError': True}, {'phase': 'received', 'method': 'initialize'}])
        self.assertEqual(result['attempts'], 1)

    def test_elapsed_capacity_refuses_late_observations(self):
        result = self.run_logic(events=[{'phase': 'received', 'method': 'initialize', 'timeMs': 600001}])
        self.assertEqual(len(result['messages']), 1)
        self.assertEqual(result['messages'][0]['value']['timeMs'], 0)


if __name__ == '__main__':
    unittest.main()
