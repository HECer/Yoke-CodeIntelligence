BEGIN ISOLATION LEVEL SERIALIZABLE READ ONLY DEFERRABLE;
SELECT pg_catalog.json_build_object('section','meta','contract','native_postgres_catalog_rows_v1',
  'database',pg_catalog.current_database(),'user',CURRENT_USER,
  'serverVersionNum',pg_catalog.current_setting('server_version_num')::pg_catalog.int4,
  'readOnly',pg_catalog.current_setting('transaction_read_only')='on',
  'isolation',pg_catalog.current_setting('transaction_isolation'),
  'clientEncoding',pg_catalog.current_setting('client_encoding'),
  'tls',COALESCE((SELECT ssl FROM pg_catalog.pg_stat_ssl WHERE pid=pg_catalog.pg_backend_pid()),false));
SELECT pg_catalog.json_build_object('section','relation','oid',c.oid::pg_catalog.int8,
  'schema',n.nspname::pg_catalog.text,'name',c.relname::pg_catalog.text,'kind',c.relkind::pg_catalog.text,
  'definitionStatus',CASE WHEN c.relkind IN ('v','m') THEN
    CASE WHEN NOT pg_catalog.pg_has_role(c.relowner,'USAGE') THEN 'not_visible_to_role'
      WHEN pg_catalog.octet_length(w.ev_action::pg_catalog.text)>__CELL_BYTES__ THEN 'exceeds_cell_limit'
      WHEN w.ev_action IS NULL THEN 'not_stored' ELSE 'available' END
    ELSE 'not_applicable' END,
  'definition',CASE WHEN c.relkind IN ('v','m') AND pg_catalog.pg_has_role(c.relowner,'USAGE')
    AND pg_catalog.octet_length(w.ev_action::pg_catalog.text)<=__CELL_BYTES__
    THEN w.ev_action::pg_catalog.text ELSE NULL END,
  'definitionKind',CASE WHEN c.relkind IN ('v','m') THEN 'pg_node_tree' ELSE NULL END)
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
LEFT JOIN pg_catalog.pg_rewrite w ON w.ev_class=c.oid AND w.rulename='_RETURN'
WHERE c.relkind IN ('r','p','f','v','m') AND n.nspname<>'information_schema' AND n.nspname!~'^pg_'
  AND pg_catalog.has_table_privilege(c.oid,'SELECT')
ORDER BY c.oid LIMIT __RELATION_ROWS__;
SELECT pg_catalog.json_build_object('section','routine','oid',p.oid::pg_catalog.int8,
  'schema',n.nspname::pg_catalog.text,'name',p.proname::pg_catalog.text,'kind',p.prokind::pg_catalog.text,
  'language',l.lanname::pg_catalog.text,'argumentTypeOids',p.proargtypes::pg_catalog.oid[],
  'definitionStatus',CASE WHEN NOT pg_catalog.pg_has_role(p.proowner,'USAGE') THEN 'not_visible_to_role'
    WHEN pg_catalog.octet_length(COALESCE(NULLIF(p.prosrc,''),p.prosqlbody::pg_catalog.text))>__CELL_BYTES__ THEN 'exceeds_cell_limit'
    WHEN COALESCE(NULLIF(p.prosrc,''),p.prosqlbody::pg_catalog.text) IS NULL THEN 'not_stored' ELSE 'available' END,
  'definition',CASE WHEN pg_catalog.pg_has_role(p.proowner,'USAGE')
    AND pg_catalog.octet_length(COALESCE(NULLIF(p.prosrc,''),p.prosqlbody::pg_catalog.text))<=__CELL_BYTES__
    THEN COALESCE(NULLIF(p.prosrc,''),p.prosqlbody::pg_catalog.text) ELSE NULL END,
  'definitionKind',CASE WHEN NULLIF(p.prosrc,'') IS NOT NULL THEN 'pg_proc.prosrc' ELSE 'pg_node_tree' END)
FROM pg_catalog.pg_proc p
JOIN pg_catalog.pg_namespace n ON n.oid=p.pronamespace
JOIN pg_catalog.pg_language l ON l.oid=p.prolang
WHERE p.prokind IN ('f','p','w') AND n.nspname<>'information_schema' AND n.nspname!~'^pg_'
  AND pg_catalog.has_function_privilege(p.oid,'EXECUTE')
ORDER BY p.oid LIMIT __ROUTINE_ROWS__;
SELECT pg_catalog.json_build_object('section','foreignKey','oid',k.oid::pg_catalog.int8,'name',k.conname::pg_catalog.text,
  'sourceOid',a.oid::pg_catalog.int8,'targetOid',b.oid::pg_catalog.int8,
  'sourceSchema',an.nspname::pg_catalog.text,'sourceName',a.relname::pg_catalog.text,
  'targetSchema',bn.nspname::pg_catalog.text,'targetName',b.relname::pg_catalog.text,
  'columns',ARRAY(SELECT t.attname::pg_catalog.text FROM pg_catalog.unnest(k.conkey) WITH ORDINALITY AS z(num,pos)
    JOIN pg_catalog.pg_attribute t ON t.attrelid=k.conrelid AND t.attnum=z.num ORDER BY z.pos),
  'targetColumns',ARRAY(SELECT t.attname::pg_catalog.text FROM pg_catalog.unnest(k.confkey) WITH ORDINALITY AS z(num,pos)
    JOIN pg_catalog.pg_attribute t ON t.attrelid=k.confrelid AND t.attnum=z.num ORDER BY z.pos),
  'deferrable',k.condeferrable,'deferred',k.condeferred,'validated',k.convalidated)
FROM pg_catalog.pg_constraint k
JOIN pg_catalog.pg_class a ON a.oid=k.conrelid
JOIN pg_catalog.pg_namespace an ON an.oid=a.relnamespace
JOIN pg_catalog.pg_class b ON b.oid=k.confrelid
JOIN pg_catalog.pg_namespace bn ON bn.oid=b.relnamespace
WHERE k.contype='f' AND an.nspname<>'information_schema' AND an.nspname!~'^pg_'
  AND pg_catalog.has_table_privilege(a.oid,'SELECT')
ORDER BY k.oid LIMIT __FOREIGN_KEY_ROWS__;
SELECT pg_catalog.json_build_object('section','end','contract','native_postgres_catalog_rows_v1',
  'readOnly',pg_catalog.current_setting('transaction_read_only')='on');
ROLLBACK;
