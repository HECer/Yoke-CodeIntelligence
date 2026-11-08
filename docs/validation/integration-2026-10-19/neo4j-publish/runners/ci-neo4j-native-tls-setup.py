from pathlib import Path
temp=Path.home()/'AppData/Local/Temp'
source=(temp/'ci-neo4j-baseline-r3.py').read_text().replace('neo4j-publish/baseline-r3','neo4j-publish/native-tls-r1')
source=source.replace("password=secrets.token_urlsafe(32)","password=secrets.token_urlsafe(24)+' '+chr(0x1f4a1)")
source=source.replace(" http=port();bolt=port();assert http!=bolt", " http=port();bolt=port();https=port();assert len({http,bolt,https})==3")
before_settings=""" certgen=run([primary/'target/debug/yoke-owned-pg-fixture-certgen.exe',root]);assert certgen.returncode==0
 public_root=(root/'root.pem').read_text();root_der_base64=base64.b64encode(ssl.PEM_cert_to_DER_cert(public_root)).decode()
 (out/'root.pem').write_text(public_root,encoding='utf-8');(out/'server.pem').write_bytes((root/'server.pem').read_bytes())
"""
source=source.replace(" settings={'server.directories.data'",before_settings+" settings={'server.directories.data'")
source=source.replace(" for value in ['data','logs'", " settings.update({'server.https.enabled':'true','server.https.listen_address':f'127.0.0.1:{https}','server.https.advertised_address':f'localhost:{https}','dbms.ssl.policy.https.enabled':'true','dbms.ssl.policy.https.base_directory':root,'dbms.ssl.policy.https.private_key':'server.key','dbms.ssl.policy.https.public_certificate':'server.pem','dbms.ssl.policy.https.client_auth':'NONE'})\n for value in ['data','logs'")
start=source.index("   marker='owned-'");end=source.index('  finally:\n   if child.poll()',start)
block=(temp/'ci-neo4j-native-live-block.txt').read_text()
block=block.replace('def workflow(prefix,selected_port,lose_response=False):','def workflow(prefix,selected_port,lose_response=False,tls=False):')
block=block.replace("config=folder/'target.json';config.write_text", "config=folder/'target.json';target.update({'baseUrl':f'https://localhost:{selected_port}/','rootCertificatesDerBase64':[root_der_base64]} if tls else {});config.write_text")
block=block.replace("connect=['--allow-network','--allow-credential-lookup','--allow-loopback-http']", "connect=['--allow-network','--allow-credential-lookup']+([] if tls else ['--allow-loopback-http'])")
block=block.replace("    direct=workflow('direct-native',http)", """    direct=workflow('direct-native',http)
    tls_owner=run(['powershell.exe','-NoProfile','-NonInteractive','-Command',f'Get-NetTCPConnection -LocalPort {https} -State Listen | Select-Object -ExpandProperty OwningProcess']);assert child.pid in [int(v) for v in tls_owner.stdout.decode().split()];(out/'https-listener-owner.txt').write_bytes(tls_owner.stdout)
    encrypted=workflow('tls-native',https,tls=True)
    bad_ca=root/'bad-ca';bad_ca.mkdir();generated=run([primary/'target/debug/yoke-owned-pg-fixture-certgen.exe',bad_ca]);assert generated.returncode==0
    wrong_root=base64.b64encode(ssl.PEM_cert_to_DER_cert((bad_ca/'root.pem').read_text())).decode()
    for label,hostname,trust in [('wrong-name','wrong.invalid',root_der_base64),('wrong-root','localhost',wrong_root)]:
     folder=out/label;folder.mkdir();target={'baseUrl':f'https://{hostname}:{https}/','address':f'127.0.0.1:{https}','database':'neo4j','namespace':'owned negative TLS','user':'neo4j','useSystemRoots':False,'rootCertificatesDerBase64':[trust]};config=folder/'target.json';config.write_text(json.dumps(target),encoding='utf-8');path=folder/'plan.json'
     cli(label+'-plan',['neo4j-publish','bootstrap-plan','--config',config,'--output',path,'--allow-output']);inspection=cli(label+'-inspect',['neo4j-publish','inspect','--plan',path]);binding=inspection['target']['credentialBinding'];cli(label+'-credential',['neo4j-publish','import-password','--config',config,'--allow-credential-create'],password.encode());created_credentials.append('yoke-code-intelligence/service/neo4j/'+binding)
     denied=cli(label+'-probe-denied',['neo4j-publish','probe','--plan',path,'--allow-network','--allow-credential-lookup'],success=False);assert 'transport_failed' in denied
""")
block=block.replace("'nativeTlsTested':False", "'nativeTlsTested':True,'nativeTlsWrongHostnameRefused':True,'nativeTlsWrongStaticRootRefused':True,'unicodePasswordViaPrivateStdin':True,'tls':encrypted")
source=source[:start]+block+'\n'+source[end:]
source=source.replace("   result_stop=child.poll() is not None", "   result_stop=child.poll() is not None\n   (out/'server-cleanup.json').write_text(json.dumps({'ownedPid':child.pid,'ownedServerStopped':result_stop})+'\\n')")
(temp/'ci-neo4j-native-tls.py').write_text(source)
