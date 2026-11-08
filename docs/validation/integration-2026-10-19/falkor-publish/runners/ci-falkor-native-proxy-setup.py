from pathlib import Path
import py_compile
p=Path(__file__).parent;s=(p/'ci-falkor-native-live.py').read_text(encoding='utf-8')
start=s.index("    native=root/'target/debug/yoke.exe'");s=s[:start]+(p/'ci-falkor-native-proxy-class.txt').read_text(encoding='utf-8')+s[start:]
s=s.replace("('tls',tls_port,True)]", "('tls',tls_port,True),('proxy',proxy_port,False)]")
old="      do_apply(prefix+'-publication-apply','apply',path,plan,journal,connect);do_apply(prefix+'-publication-idempotent-journal','apply',path,plan,journal,connect)"
new="""      if prefix=='proxy':
       drop_next['armed']=True;denied=do_apply(prefix+'-publication-response-lost','apply',path,plan,journal,connect,False);assert 'host_operation_failed' in denied
       historical_dir=dest/'historical-pending';historical_dir.mkdir();historical=historical_dir/journal.name;historical.write_bytes(journal.read_bytes());assert json.loads(journal.read_bytes().splitlines()[-1])['phase']=='pending'
       before=len(proxy_events);denied=do_apply(prefix+'-unknown-result-resend-blocked','apply',path,plan,journal,connect,False);assert 'previous_result_unknown_recovery_required' in denied and not any(e['publication'] for e in proxy_events[before:])
       do_apply(prefix+'-lost-result-read-recovery','recover',path,plan,journal,connect);assert not any(e['publication'] for e in proxy_events[before:])
      else:do_apply(prefix+'-publication-apply','apply',path,plan,journal,connect)
      do_apply(prefix+'-publication-idempotent-journal','apply',path,plan,journal,connect)"""
assert old in s;s=s.replace(old,new)
needle="     for label,hostname,root_der in "
start=s.index(needle);s=s[:start]+(p/'ci-falkor-native-proxy-extension.txt').read_text(encoding='utf-8')+s[start:]
s=s.replace("    finally:\n     removed=0;", "    finally:\n     proxy.shutdown();proxy.server_close();proxy_thread.join(timeout=3);(out/'proxy-events.json').write_text(json.dumps(proxy_events,indent=2)+'\\n',encoding='utf-8')\n     removed=0;")
s=s.replace("'lostStdoutIdentityRetained':True,", "'lostStdoutIdentityRetained':True,'responseLostAfterActualCommit':True,'forcedConcurrentUpdatesExactlyOneCommit':True,'loserCreatedNoRowsOrCounterChanges':True,'historicalReadRecoveryAfterLaterActive':True,'wrongSchemaRefusedWithoutRepair':True,")
dest=p/'ci-falkor-native-live.py';dest.write_text(s,encoding='utf-8');py_compile.compile(str(dest),doraise=True);print('Prepared owned response-loss/CAS proxy; no server started')
