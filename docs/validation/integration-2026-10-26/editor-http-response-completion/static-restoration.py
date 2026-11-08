from pathlib import Path
import hashlib,json,subprocess
root=Path.cwd()
path="native/src/editor_http.rs"
old=subprocess.check_output(["git","show","52b746dcb313e3f0f26a3d4538456cb1285f6c5e:"+path],timeout=5).decode("utf-8")
new=(root/path).read_text(encoding="utf-8")
restored=new
def replace_once(before,after):
    global restored
    assert restored.count(before)==1, before[:80]
    restored=restored.replace(before,after,1)
replace_once("""// Tests distinguish the selected HTTP status from a returned transport failure.
// The non-test result and both existing success paths remain unchanged.
#[cfg(test)]
struct ResponseCompletion {
    status: u16,
    serialization_succeeded: bool,
    serialized_bytes: usize,
}
#[cfg(not(test))]
type ResponseCompletion = ();

fn respond(
    stream: &mut TcpStream,
    response: Response,
    deadline: &Deadline,
) -> HttpResult<ResponseCompletion> {""",
"""fn respond(stream: &mut TcpStream, response: Response, deadline: &Deadline) -> HttpResult<()> {""")
replace_once("""    #[cfg(test)]
    let serialization = (serialized.is_ok(), writer.bytes.len());
""","")
replace_once("""    #[cfg(test)]
    let completion = ResponseCompletion {
        status,
        serialization_succeeded: serialization.0,
        serialized_bytes: serialization.1,
    };
""","")
replace_once("""        #[cfg(test)]
        return Ok(completion);
        #[cfg(not(test))]
""","")
replace_once("""    #[cfg(test)]
    {
        Ok(completion)
    }
    #[cfg(not(test))]
    {
        Ok(())
    }
""","""    Ok(())
""")
old_prefix=old.split("#[cfg(test)]\nmod tests {",1)[0]
new_prefix=restored.split("#[cfg(test)]\nmod tests {",1)[0]
assert new_prefix==old_prefix
replace_once("""            let completion = result.as_ref().ok().map(|receipt| {
                (
                    receipt.status,
                    receipt.serialization_succeeded,
                    receipt.serialized_bytes,
                )
            });
""","")
replace_once("sender.send((timeout, started.elapsed(), failure_status, completion))","sender.send((timeout, started.elapsed(), failure_status))")
replace_once("""        let (timeout, elapsed, failure_status, completion) =
            result.expect("Writer must stop at its deadline");""","""        let (timeout, elapsed, failure_status) = result.expect("Writer must stop at its deadline");""")
replace_once(", completion(status,serialization_succeeded,serialized_bytes)={completion:?}","")
assert restored==old
print(json.dumps({"staticSourceRestoration":True,"productionPrefixRestoredExactly":True,"originalDeadlinePayloadSocketAndAssertionsRetained":True,"sourcePath":path,"baselineCommit":"52b746dcb313e3f0f26a3d4538456cb1285f6c5e","baselineGitBlobSha256":hashlib.sha256(old.encode()).hexdigest(),"newNormalizedSourceSha256":hashlib.sha256(new.encode()).hexdigest(),"nativeCompilationOrExecution":False}))
