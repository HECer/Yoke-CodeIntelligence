from pathlib import Path
temp=Path.home()/'AppData/Local/Temp'
source=(temp/'ci-neo4j-native-tls.py').read_text().replace('neo4j-publish/native-tls-r1','neo4j-publish/native-tls-r2')
source=source.replace("password=secrets.token_urlsafe(24)+' '+chr(0x1f4a1)","password=secrets.token_urlsafe(32)")
block="""   query('fixture-ascii-auth-ready','RETURN 1 AS ready')
   initial_password=password;initial_auth=auth;unicode_password=password+' '+chr(0x1f4a1)
   private_body=json.dumps({'statement':'CYPHER 5 ALTER CURRENT USER SET PASSWORD FROM $oldPassword TO $newPassword','parameters':{'oldPassword':initial_password,'newPassword':unicode_password}},ensure_ascii=False).encode()
   private_request=urllib.request.Request(endpoint.replace('/db/neo4j/','/db/system/'),data=private_body,headers={'Authorization':initial_auth,'Content-Type':'application/json'},method='POST')
   try:
    with opener.open(private_request,timeout=30) as changed:status=changed.status;private_result=changed.read(128000)
   except urllib.error.HTTPError as error:status=error.code;private_result=error.read(128000)
   assert all(secret.encode() not in private_result for secret in [initial_password,unicode_password,initial_auth])
   (out/'private-password-setup.json').write_text(json.dumps({'status':status,'responseHash':hashlib.sha256(private_result).hexdigest(),'success':status==202 and not json.loads(private_result).get('errors'),'passwordTransport':'private_UTF8_parameterized_Query_API_owned_fixture_only'})+'\\n')
   assert status==202 and not json.loads(private_result).get('errors'),'Owned private password setup failed; values withheld'
   password=unicode_password;auth='Basic '+base64.b64encode(('neo4j:'+password).encode()).decode()
   query('fixture-unicode-api-auth-ready','RETURN $text AS text',{'text':'owned '+chr(0x1f4a1)})
"""
source=source.replace('   import threading\n',block+'   import threading\n')
source=source.replace("assert password.encode() not in artifact.read_bytes() and auth.encode() not in artifact.read_bytes(),'Private fixture authentication leaked'", "assert all(s.encode() not in artifact.read_bytes() for s in [password,auth,initial_password,initial_auth]),'Private fixture authentication leaked'")
(temp/'ci-neo4j-native-tls-r2.py').write_text(source)
