"""Execute only the pure statistics model; no dashboard/native worker is imported."""
import json
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
HTML = ROOT / 'native/src/session_dashboard.html'
START = '/* native-tool-usage-model:start */'
END = '/* native-tool-usage-model:end */'
DRIVER = r"""
const vm=require('node:vm');
const input=JSON.parse(require('node:fs').readFileSync(0,'utf8'));
const context=vm.createContext({input});
try {
 vm.runInContext(input.source,context,{timeout:1000});
 const value=vm.runInContext(input.expression,context,{timeout:1000});
 process.stdout.write(JSON.stringify({ok:true,value},(_,v)=>typeof v==='bigint'?v.toString():v));
} catch { process.stdout.write(JSON.stringify({ok:false})); }
"""


def fixture():
    return {'schema': 'native_tool_usage_display_v1', 'estimator': 'CHAR_COUNT',
            'scope': 'current_session_instance_successful_serena_operations',
            'revision': '3', 'hostGrantRemoteAttempts': '0',
            'tools': {'read_file': {'numTimesCalled': '9007199254740993',
                                   'inputTokens': '18446744073709551615', 'outputTokens': '7'},
                      'search': {'numTimesCalled': '1', 'inputTokens': '0', 'outputTokens': '0'}}}


@unittest.skipUnless(shutil.which('node'), 'Node is needed for the pure dashboard model')
class DashboardToolUsage(unittest.TestCase):
    def run_model(self, data, expression='usageModel(input.data)'):
        html = HTML.read_text(encoding='utf-8')
        self.assertEqual(html.count(START), 1, 'Tool statistics model is missing')
        self.assertEqual(html.count(END), 1)
        source = html.split(START, 1)[1].split(END, 1)[0]
        self.assertNotIn('document.', source)
        self.assertNotIn('fetch(', source)
        payload = {'source': source, 'expression': expression, 'data': data}
        output = subprocess.check_output([shutil.which('node'), '--eval', DRIVER],
                                         input=json.dumps(payload).encode(), timeout=8)
        self.assertLessEqual(len(output), 64000)
        return json.loads(output)

    def test_u64_values_totals_formatting_and_relative_width_never_round_to_double(self):
        result = self.run_model(fixture(),
            "(()=>{const m=usageModel(input.data);return {totals:m.totals,formatted:usageFormat(m.totals.calls),width:usageWidth(1n,3n),full:usageWidth(18446744073709551615n,18446744073709551615n)}})()")
        self.assertTrue(result['ok'])
        self.assertEqual(result['value']['totals'], {'calls': '9007199254740994',
                         'input': '18446744073709551615', 'output': '7', 'tokens': '18446744073709551622'})
        self.assertEqual(result['value']['formatted'], '9,007,199,254,740,994')
        self.assertEqual(result['value']['width'], '33.33%')
        self.assertEqual(result['value']['full'], '100%')

    def test_missing_unknown_and_unsafe_numeric_statistics_refuse_instead_of_zero(self):
        variants = [None, {}, {**fixture(), 'schema': 'other'}, {**fixture(), 'estimator': 'unknown'},
                    {**fixture(), 'extra': 'provider content'}]
        for value in ('00', '+1', '1e3', '１２', '', None, 0, '18446744073709551616'):
            data = fixture()
            data['tools']['read_file']['inputTokens'] = value
            variants.append(data)
        for data in variants:
            with self.subTest(data=data):
                self.assertFalse(self.run_model(data)['ok'])

    def test_selected_empty_statistics_are_real_zero_and_do_not_invent_usage(self):
        data = fixture()
        data['tools'] = {}
        result = self.run_model(data)
        self.assertTrue(result['ok'])
        self.assertEqual(result['value']['rows'], [])
        self.assertEqual(set(result['value']['totals'].values()), {'0'})

    def test_tool_capacity_names_and_closed_entry_fields_are_checked(self):
        data = fixture()
        data['tools'] = {f'tool_{n}': {'numTimesCalled': '1', 'inputTokens': '0', 'outputTokens': '0'}
                         for n in range(97)}
        self.assertFalse(self.run_model(data)['ok'])
        for name in ('', '🌍', '<img src=x onerror=alert(1)>', 't' * 101):
            data = fixture()
            data['tools'] = {name: {'numTimesCalled': '1', 'inputTokens': '0', 'outputTokens': '0'}}
            self.assertFalse(self.run_model(data)['ok'])
        data = fixture()
        data['tools']['read_file']['contents'] = 'source text'
        self.assertFalse(self.run_model(data)['ok'])

    def test_filter_sort_and_prototype_spelling_are_data_without_mutation(self):
        data = fixture()
        data['tools']['__proto__'] = {'numTimesCalled': '2', 'inputTokens': '0', 'outputTokens': '0'}
        result = self.run_model(data,
            "(()=>{const m=usageModel(input.data);return {all:usageRows(m,'','calls').map(r=>r.name),filtered:usageRows(m,'READ','tokens').map(r=>r.name),unmodified:m.rows.length,prototypeClean:({}).numTimesCalled===undefined}})()")
        self.assertTrue(result['ok'])
        self.assertEqual(result['value']['all'], ['read_file', '__proto__', 'search'])
        self.assertEqual(result['value']['filtered'], ['read_file'])
        self.assertEqual(result['value']['unmodified'], 3)
        self.assertTrue(result['value']['prototypeClean'])
        self.assertFalse(self.run_model(data, "usageRows(usageModel(input.data),'x'.repeat(101),'tokens')")['ok'])


@unittest.skipUnless(shutil.which('node'), 'Node is needed for the pure refresh orchestration')
class DashboardRefresh(unittest.TestCase):
    def run_refresh(self, expression):
        html = HTML.read_text(encoding='utf-8')
        start = '/* native-dashboard-refresh:start */'
        end = '/* native-dashboard-refresh:end */'
        self.assertEqual(html.count(start), 1, 'Bound refresh orchestration is missing')
        source = html.split(start, 1)[1].split(end, 1)[0]
        prelude = r"""
let ending=false,managementBusy=false,token='a'.repeat(64);
const requests=[],rendered=[],states=[];
function fetch(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b});requests.push({resolve,reject});return promise}
function render(value){rendered.push(value.view.revision)}
function state(ok){states.push(ok)}
function response(revision){return {ok:true,json:async()=>({schema:'native_session_dashboard_v1',view:{revision}})}}
const tick=()=>new Promise(resolve=>setImmediate(resolve));
"""
        driver = r"""
const vm=require('node:vm'),fs=require('node:fs');
const input=JSON.parse(fs.readFileSync(0,'utf8'));
const context=vm.createContext({AbortController,setTimeout:()=>1,clearTimeout:()=>{},setImmediate});
(async()=>{
 try{const value=await vm.runInContext(input.source+'\n'+input.expression,context,{timeout:1000});process.stdout.write(JSON.stringify({ok:true,value}))}
 catch(error){process.stdout.write(JSON.stringify({ok:false,error:String(error)}))}
})();
"""
        output = subprocess.check_output([shutil.which('node'), '--eval', driver],
            input=json.dumps({'source': prelude + source, 'expression': expression}).encode(), timeout=8)
        result = json.loads(output)
        self.assertTrue(result['ok'], result)
        return result['value']

    def test_refresh_callers_join_one_pending_read(self):
        self.assertEqual(self.run_refresh(r"""(async()=>{
 const first=refresh(),second=refresh();const started=requests.length;
 requests[0].resolve(response(1));return {started,result:await Promise.all([first,second]),rendered};
})()"""), {'started': 1, 'result': [True, True], 'rendered': [1]})

    def test_pre_management_read_cannot_republish_old_counts(self):
        self.assertEqual(self.run_refresh(r"""(async()=>{
 const old=refresh();++refreshGeneration;const fresh=refreshAfterManagement();
 requests[0].resolve(response(1));await old;await tick();
 const before=rendered.slice();requests[1].resolve(response(2));
 return {before,result:await fresh,rendered,requests:requests.length};
})()"""), {'before': [], 'result': True, 'rendered': [2], 'requests': 2})

    def test_obsolete_errors_and_busy_management_do_not_publish(self):
        self.assertEqual(self.run_refresh(r"""(async()=>{
 const old=refresh();++refreshGeneration;managementBusy=true;
 requests[0].reject(Error('obsolete'));await old;
 const refused=await refresh();managementBusy=false;
 const fresh=refreshAfterManagement();await tick();requests[1].resolve(response(3));await fresh;
 return {refused,states,rendered,requests:requests.length};
})()"""), {'refused': False, 'states': [True], 'rendered': [3], 'requests': 2})


if __name__ == '__main__':
    unittest.main()
