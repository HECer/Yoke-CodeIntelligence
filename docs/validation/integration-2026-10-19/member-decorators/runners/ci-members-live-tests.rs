
fn actual_cycle(source:&str,expected:&str,path:&str,through_provider:bool) {
    let node=std::env::var("YOKE_NODE").expect("Select actual Node executable");
    let compiler=std::env::var("YOKE_TSC").expect("Select actual TypeScript compiler");
    let library=std::path::Path::new(&compiler).parent().unwrap().join("typescript.js");
    for selected in [std::path::Path::new(&node),std::path::Path::new(&compiler),library.as_path()] {
        assert!(selected.is_absolute()&&selected.is_file());
    }
    let root=tempfile::tempdir().unwrap();fs::write(root.path().join(path),source).unwrap();
    fs::write(root.path().join("tsconfig.json"),serde_json::to_vec(&json!({"compilerOptions":{
        "target":"es2022","module":"esnext","strict":true,"noEmit":true,"experimentalDecorators":true,
        "allowJs":true,"checkJs":true,"types":[],"skipLibCheck":true},"files":[path]})).unwrap()).unwrap();
    let mut engine=Engine::open(root.path()).unwrap();let(snapshot,graph,_,_)=engine.index(&d()).unwrap();
    let selected=graph.entities.iter().find(|e|e.name=="deleteMe").unwrap();let original=serde_json::to_value(selected).unwrap();
    let plan=if through_provider {
        let cli=std::env::var("YOKE_TS_LSP").expect("Select actual TypeScript language-server CLI");
        assert!(std::path::Path::new(&cli).is_absolute()&&std::path::Path::new(&cli).is_file());
        engine.register_configured_server(yoke::lsp::ConfiguredServer{
            server:yoke::lsp::Server{command:node.clone(),args:vec![cli,"--stdio".into()],language:"typescript".into(),trust:true},
            configuration:yoke::lsp::HostConfiguration{passive_profile:Some(yoke::lsp::PassiveProfile::TypescriptLanguageServer),..Default::default()}
        }).unwrap();
        let value=yoke::semantic_refactor::invoke(&mut engine,"safeDelete",&json!({"relativePath":path,"namePath":"deleteMe"}),&Deadline::new(90_000)).unwrap();
        println!("actual-member-safe-delete={value}");assert_eq!(value["complete"],false);
        serde_json::from_value(value["plan"].clone()).unwrap()
    } else {
        changes::prepare(&mut engine.store,&snapshot,&graph,&json!({"deleteSymbol":{"entityId":selected.id}}),&d()).unwrap()
    };
    let args=json!({"planId":plan.id,"idempotencyKey":"actual-member-decorators","mode":"workspace"});
    assert!(changes::apply(&engine.store,root.path(),&args,true,&d()).is_err());
    let compile=yoke::verify::execute(&engine,&plan.id,&node,&[compiler,"--noEmit".into(),"--project".into(),"tsconfig.json".into()],true,&Deadline::new(60_000)).unwrap();
    println!("actual-member-{path}-compiler={compile}");assert_eq!(compile["status"],"passed","{compile}");
    let ast_code="const ts=require(process.argv[1]); const fs=require('fs'); const file=process.argv[2]; const kind=file.endsWith('.tsx')?ts.ScriptKind.TSX:file.endsWith('.jsx')?ts.ScriptKind.JSX:file.endsWith('.js')?ts.ScriptKind.JS:ts.ScriptKind.TS; const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true,kind); const cls=tree.statements.find(ts.isClassDeclaration); if(!cls)throw Error('class missing'); const methods=cls.members.filter(ts.isMethodDeclaration); if(methods.length!==1||methods[0].name.getText(tree)!=='kept')throw Error('method selection changed'); if((ts.getDecorators(methods[0])||[]).length!==0)throw Error('decorator reattached to neighbor'); const parent=ts.getDecorators(cls)||[]; if(parent.length!==1||parent[0].expression.getText(tree)!=='parentMark')throw Error('parent decorator changed'); console.log('checked-parent-and-neighbor-decorator-ownership')";
    let ast=yoke::verify::execute(&engine,&plan.id,&node,&["-e".into(),ast_code.into(),library.to_str().unwrap().into(),path.into()],true,&d()).unwrap();
    println!("actual-member-{path}-ast={ast}");assert_eq!(ast["status"],"passed","{ast}");
    assert!(changes::apply(&engine.store,root.path(),&args,true,&d()).is_err());
    changes::approve(&engine.store,&plan.id,false).unwrap();
    assert!(changes::apply(&engine.store,root.path(),&args,false,&d()).is_err());
    let receipt=changes::apply(&engine.store,root.path(),&args,true,&d()).unwrap();
    assert_eq!(fs::read(root.path().join(path)).unwrap(),expected.as_bytes());
    changes::recover(&engine.store,root.path(),&receipt.id,true).unwrap();
    assert_eq!(fs::read(root.path().join(path)).unwrap(),source.as_bytes());
    assert_eq!(query::source(&engine.store,&snapshot,path).unwrap().0,source);
    assert_eq!(serde_json::to_value(selected).unwrap(),original);
}
#[test]
#[ignore="requires YOKE_NODE and YOKE_TSC selecting actual installed compiler/AST tools"]
fn actual_typescript_compiler_and_ast_confirm_parent_and_neighbor_decorator_ownership(){
    for extension in ["ts","tsx","js","jsx"] {
        let prefix=if matches!(extension,"ts"|"tsx") {
            "declare const parentMark: any, firstMark: any, secondMark: any;\r\n"
        } else {"/** @type {any} */ var parentMark; /** @type {any} */ var firstMark; /** @type {any} */ var secondMark;\r\n"};
        let source=format!("\u{feff}// café 🦀\r\n{prefix}@parentMark\r\nexport class Parent {{\r\n    @firstMark\r\n    @secondMark\r\n    deleteMe() {{ return 1; }}\r\n    kept() {{ return 2; }}\r\n}}\r\n");
        let expected=format!("\u{feff}// café 🦀\r\n{prefix}@parentMark\r\nexport class Parent {{\r\n    \r\n    kept() {{ return 2; }}\r\n}}\r\n");
        actual_cycle(&source,&expected,&format!("module.{extension}"),false);
    }
}
#[test]
#[ignore="requires YOKE_NODE/YOKE_TS_LSP/YOKE_TSC selecting actual TypeScript language server and compiler"]
fn actual_language_server_safe_delete_uses_the_same_checked_member_decorator_plan(){
    let source="\u{feff}// café 🦀\r\ndeclare const parentMark: any, firstMark: any, secondMark: any;\r\n@parentMark\r\nexport class Parent {\r\n    @firstMark\r\n    @secondMark\r\n    deleteMe() { return 1; }\r\n    kept() { return 2; }\r\n}\r\n";
    let expected="\u{feff}// café 🦀\r\ndeclare const parentMark: any, firstMark: any, secondMark: any;\r\n@parentMark\r\nexport class Parent {\r\n    \r\n    kept() { return 2; }\r\n}\r\n";
    actual_cycle(source,expected,"module.ts",true);
}
