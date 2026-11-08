
#[test]
#[ignore = "requires YOKE_NODE and YOKE_TSC selecting actual Node/TypeScript compiler"]
fn actual_typescript_compiles_preserved_ts_js_tsx_jsx_siblings_then_recovers_exact_source() {
    let node=std::env::var("YOKE_NODE").expect("Select actual Node executable");
    let compiler=std::env::var("YOKE_TSC").expect("Select actual TypeScript compiler CLI");
    for path in [&node,&compiler] { assert!(std::path::Path::new(path).is_absolute()&&std::path::Path::new(path).is_file()); }
    for extension in ["ts","js","tsx","jsx"] {
        let root=tempfile::tempdir().unwrap();let path=format!("module.{extension}");
        let jsx=matches!(extension,"tsx"|"jsx");
        let (source,expected)=if jsx {
            ("\u{feff}// café 🦀\r\nexport const kept = () => <span/>, deleteMe = () => <div/>, last = () => <span/>;\r\n",
             "\u{feff}// café 🦀\r\nexport const kept = () => <span/>,  last = () => <span/>;\r\n")
        } else {
            ("\u{feff}// café 🦀\r\nexport const kept = () => 'kept', deleteMe = (value = ',🦀,') => value, last = () => 'last';\r\n",
             "\u{feff}// café 🦀\r\nexport const kept = () => 'kept',  last = () => 'last';\r\n")
        };
        fs::write(root.path().join(&path),source).unwrap();
        fs::write(root.path().join("jsx.d.ts"),"declare namespace JSX { interface IntrinsicElements { span: {}; div: {}; } }\n").unwrap();
        fs::write(root.path().join("tsconfig.json"),serde_json::to_vec(&json!({"compilerOptions":{
            "target":"es2022","module":"esnext","strict":true,"noEmit":true,"jsx":"preserve",
            "allowJs":true,"checkJs":true,"types":[],"skipLibCheck":true},"files":[path,"jsx.d.ts"]})).unwrap()).unwrap();
        let mut engine=Engine::open(root.path()).unwrap();let(snapshot,graph,_,_)=engine.index(&d()).unwrap();
        let selected=graph.entities.iter().find(|e|e.name=="deleteMe").unwrap();
        let original=serde_json::to_value(selected).unwrap();
        let plan=changes::prepare(&mut engine.store,&snapshot,&graph,
            &json!({"deleteSymbol":{"entityId":selected.id,"siblingDeclarators":"preserve"}}),&d()).unwrap();
        let args=json!({"planId":plan.id,"idempotencyKey":"actual-preserved-declarators","mode":"workspace"});
        assert!(changes::apply(&engine.store,root.path(),&args,true,&d()).is_err());
        let verification=yoke::verify::execute(&engine,&plan.id,&node,
            &[compiler.clone(),"--noEmit".into(),"--project".into(),"tsconfig.json".into()],true,&Deadline::new(60_000)).unwrap();
        println!("actual-{extension}-sibling-verification={verification}");
        assert_eq!(verification["status"],"passed","{verification}");
        assert!(changes::apply(&engine.store,root.path(),&args,true,&d()).is_err());
        changes::approve(&engine.store,&plan.id,false).unwrap();
        assert!(changes::apply(&engine.store,root.path(),&args,false,&d()).is_err());
        let receipt=changes::apply(&engine.store,root.path(),&args,true,&d()).unwrap();
        assert_eq!(fs::read(root.path().join(&path)).unwrap(),expected.as_bytes());
        changes::recover(&engine.store,root.path(),&receipt.id,true).unwrap();
        assert_eq!(fs::read(root.path().join(&path)).unwrap(),source.as_bytes());
        assert_eq!(query::source(&engine.store,&snapshot,&path).unwrap().0,source);
        assert_eq!(serde_json::to_value(selected).unwrap(),original);
        println!("actual-{extension}-sibling-plan={}",serde_json::to_value(&plan).unwrap());
    }
}
