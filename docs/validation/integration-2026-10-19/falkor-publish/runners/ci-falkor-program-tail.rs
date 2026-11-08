fn literal(value:&Value,out:&mut String,depth:usize,d:&Deadline)->Result<()>{
    d.check()?;ensure!(depth<=16,"falkor_publish_parameter_capacity_exceeded");
    match value{
        Value::Null=>out.push_str("null"),Value::Bool(v)=>out.push_str(if *v{"true"}else{"false"}),
        Value::Number(n)=>{ensure!(n.as_i64().is_some(),"falkor_publish_parameter_invalid");out.push_str(&n.to_string());},
        Value::String(v)=>out.push_str(&serde_json::to_string(v)?),
        Value::Array(values)=>{out.push('[');for(i,value)in values.iter().enumerate(){if i>0{out.push(',')}literal(value,out,depth+1,d)?;}out.push(']');},
        Value::Object(values)=>{out.push('{');for(i,(key,value))in values.iter().enumerate(){ensure!(!key.is_empty()&&key.bytes().enumerate().all(|(i,b)|b.is_ascii_alphabetic()||b==b'_'||(i>0&&b.is_ascii_digit())),"falkor_publish_parameter_key_invalid");if i>0{out.push(',')}out.push_str(key);out.push(':');literal(value,out,depth+1,d)?;}out.push('}');}
    }
    ensure!(out.len()<=32_000_000,"falkor_publish_parameter_capacity_exceeded");Ok(())
}
pub(crate) fn query(graph_key:&str,statement:&str,params:&Value,read_only:bool,d:&Deadline)->Result<Vec<Vec<u8>>>{
    d.check()?;crate::json::encode_bounded(params,32_000_000,d)?;let object=params.as_object().ok_or_else(||anyhow::anyhow!("falkor_publish_parameter_invalid"))?;let mut text=String::new();
    if !object.is_empty(){text.push_str("CYPHER ");for(key,value)in object{ensure!(!key.is_empty()&&key.bytes().enumerate().all(|(i,b)|b.is_ascii_alphabetic()||b==b'_'||(i>0&&b.is_ascii_digit())),"falkor_publish_parameter_key_invalid");text.push_str(key);text.push('=');literal(value,&mut text,0,d)?;text.push(' ');}}
    text.push_str(statement);ensure!(text.len()<=32_000_000,"falkor_publish_parameter_capacity_exceeded");
    let command=vec![if read_only{b"GRAPH.RO_QUERY".to_vec()}else{b"GRAPH.QUERY".to_vec()},graph_key.as_bytes().to_vec(),text.into_bytes(),b"TIMEOUT".to_vec(),d.remaining().as_millis().clamp(1,600000).to_string().into_bytes()];
    crate::falkor_wire::encode(&command.iter().map(Vec::as_slice).collect::<Vec<_>>(),&Default::default(),d)?;Ok(command)
}
pub fn command(plan:&Plan,op:Operation,d:&Deadline)->Result<Vec<Vec<u8>>>{
    plan.validate(d)?;ensure!(operations(plan.mode()).contains(&op),"falkor_publish_operation_invalid");
    let definition=match op{Operation::ScopeConstraint=>Some(("NODE","YokeNativeScope")),Operation::ElementConstraint=>Some(("NODE","YokeNativeElement")),Operation::ObservationConstraint=>Some(("RELATIONSHIP","YOKE_NATIVE_OBSERVATION")),_=>None};
    if let Some((entity,label))=definition{return Ok(["GRAPH.CONSTRAINT","CREATE",plan.graph_key(),"UNIQUE",entity,label,"PROPERTIES","1","key"].map(|s|s.as_bytes().to_vec()).to_vec())}
    let statement=match op{Operation::Initialize=>INITIALIZE.to_owned(),Operation::ScopeIndex=>INDEXES[0].into(),Operation::ElementIndex=>INDEXES[1].into(),Operation::ObservationIndex=>INDEXES[2].into(),Operation::ElementRevisionIndex=>INDEXES[3].into(),Operation::ObservationRevisionIndex=>INDEXES[4].into(),Operation::Publish=>publication(),_=>unreachable!()};
    query(plan.graph_key(),&statement,&parameters(plan,d)?,false,d)
}
