use rcgen::{CertificateParams,KeyPair,IsCa,BasicConstraints,KeyUsagePurpose,ExtendedKeyUsagePurpose,DnType};
fn main()->Result<(),Box<dyn std::error::Error>> {
    let out=std::path::PathBuf::from(std::env::args().nth(1).ok_or("output required")?);
    assert!(out.is_dir());
    let ca_key=KeyPair::generate()?;let mut p=CertificateParams::new(Vec::<String>::new())?;
    p.is_ca=IsCa::Ca(BasicConstraints::Unconstrained);p.key_usages=vec![KeyUsagePurpose::KeyCertSign,KeyUsagePurpose::DigitalSignature,KeyUsagePurpose::CrlSign];p.distinguished_name.push(DnType::CommonName,"Yoke owned PostgreSQL fixture CA");
    let ca=p.self_signed(&ca_key)?;
    let key=KeyPair::generate()?;let mut p=CertificateParams::new(vec!["localhost".into()])?;
    p.key_usages=vec![KeyUsagePurpose::DigitalSignature,KeyUsagePurpose::KeyEncipherment];p.extended_key_usages=vec![ExtendedKeyUsagePurpose::ServerAuth];p.distinguished_name.push(DnType::CommonName,"localhost");
    let cert=p.signed_by(&key,&ca,&ca_key)?;
    std::fs::write(out.join("root.pem"),ca.pem())?;std::fs::write(out.join("server.pem"),cert.pem())?;std::fs::write(out.join("server.key"),key.serialize_pem())?;
    println!("owned fixture certificate files created");Ok(())
}
