//! Explicit shell selection for Claude's shell-form statusLine contract.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceStatusShell {
    pub path: String,
    pub file: FileBinding,
}

fn shell_token(path: &str) -> Result<String> {
    input_path(Path::new(path))?;
    let token = path.replace('\\', "/");
    // Only this token and base64 arguments reach the outer host shell. Project,
    // configuration and state paths have no ASCII/space restriction.
    ensure!(
        token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b":/._-".contains(&b)),
        "Status bootstrap requires a literal ASCII shell path without shell metacharacters or spaces"
    );
    ensure!(
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(
                |n| n.eq_ignore_ascii_case("powershell.exe") || n.eq_ignore_ascii_case("pwsh.exe")
            ),
        "Status bootstrap requires selected PowerShell"
    );
    Ok(token)
}
impl Selection {
    /// Additional explicit host execution selection; no PATH or home discovery.
    pub fn with_source_status_shell(mut self, path: &Path, d: &Deadline) -> Result<Self> {
        let text = path_text(path)?;
        shell_token(&text)?;
        let file = io::read_executable_binding(path, 512_000_000, d)?
            .context("Status shell unavailable")?;
        let Some(AdapterBinding::ClaudeSourceHooksV1 {
            source_root,
            status_shell,
            ..
        }) = &mut self.bindings.adapter
        else {
            bail!("Status shell requires source hook profile")
        };
        ensure!(
            !file.path.starts_with(Path::new(&source_root.path)),
            "Status shell must be outside analyzed source"
        );
        *status_shell = Some(SourceStatusShell {
            path: text,
            file: FileBinding {
                path: path_text(&file.path)?,
                identity: file.identity.clone(),
                hash: file.hash.clone(),
                bytes: file.byte_length,
            },
        });
        drop(file);
        let _held = guard_bindings(&self, d)?;
        super::body(&self)?;
        Ok(self)
    }
}
pub(super) fn guard_status_shell(
    selection: &Selection,
    files: &mut Vec<io::HeldFile>,
    d: &Deadline,
) -> Result<()> {
    let Some(AdapterBinding::ClaudeSourceHooksV1 {
        source_root,
        status_shell: Some(shell),
        ..
    }) = &selection.bindings.adapter
    else {
        return Ok(());
    };
    shell_token(&shell.path)?;
    let file = io::read_executable_binding(Path::new(&shell.path), 512_000_000, d)?
        .context("Status shell unavailable")?;
    ensure!(
        file.identity == shell.file.identity
            && file.hash == shell.file.hash
            && file.byte_length == shell.file.bytes
            && path_text(&file.path)? == shell.file.path
            && !file.path.starts_with(Path::new(&source_root.path)),
        "Status shell binding changed"
    );
    files.push(file);
    Ok(())
}
fn literal(word: &str) -> String {
    format!("'{}'", word.replace('\'', "''"))
}
// ProcessStartInfo.Arguments goes directly to CreateProcess. PowerShell never
// re-tokenizes these native argument values (including JSON quotes/backslashes).
fn windows_word(word: &str) -> String {
    let mut result = String::from("\"");
    let mut slashes = 0;
    for c in word.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            '\\',
            if c == '"' { slashes * 2 + 1 } else { slashes },
        ));
        result.push(c);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    result
}
pub(super) fn command(selection: &Selection, launch: &str) -> Result<Option<String>> {
    let Some(AdapterBinding::ClaudeSourceHooksV1 {
        executable_path,
        status_shell: Some(shell),
        ..
    }) = &selection.bindings.adapter
    else {
        return Ok(None);
    };
    let token = shell_token(&shell.path)?;
    let argv = [
        executable_path.as_str(),
        "--timeout",
        "20000",
        "--max-bytes",
        "4096",
        "claude-source-hook",
        "status",
        "--launch-json",
        launch,
    ];
    let words = argv[1..]
        .iter()
        .map(|word| windows_word(word))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        "$ErrorActionPreference='Stop'; $p=$null; $started=$false; $code=1; try {{ $s=[System.Diagnostics.ProcessStartInfo]::new(); $s.FileName={}; $s.Arguments={}; $s.UseShellExecute=$false; $s.CreateNoWindow=$true; $s.RedirectStandardInput=$true; $s.RedirectStandardOutput=$true; $s.RedirectStandardError=$true; $p=[System.Diagnostics.Process]::new(); $p.StartInfo=$s; if(-not $p.Start()){{throw 'start'}}; $started=$true; $out=[Console]::OpenStandardOutput(); $err=[Console]::OpenStandardError(); $o=$p.StandardOutput.BaseStream.CopyToAsync($out); $e=$p.StandardError.BaseStream.CopyToAsync($err); $i=[Console]::OpenStandardInput().CopyToAsync($p.StandardInput.BaseStream); if(-not $i.Wait(20000)){{throw 'input'}}; $p.StandardInput.BaseStream.Close(); if(-not $p.WaitForExit(25000)){{throw 'wait'}}; if(-not $o.Wait(2000) -or -not $e.Wait(2000)){{throw 'output'}}; $out.Flush(); $err.Flush(); $code=$p.ExitCode }} catch {{ [Console]::Error.WriteLine('yoke_status_shell_refused') }} finally {{ if($p){{if($started -and -not $p.HasExited){{$p.Kill(); $null=$p.WaitForExit(5000)}}; $p.Dispose()}} }}; exit $code",
        literal(executable_path),
        literal(&words)
    );
    let utf16 = script
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let command = format!(
        "{token} -NoLogo -NoProfile -NonInteractive -EncodedCommand {}",
        STANDARD.encode(utf16)
    );
    ensure!(
        command.len() <= 28_000,
        "Status bootstrap command capacity exceeded"
    );
    Ok(Some(command))
}
