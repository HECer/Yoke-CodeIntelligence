use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: &'static str,
    pub provider: &'static str,
    pub languages: &'static [&'static str],
    pub extensions: &'static [&'static str],
    pub companions: &'static [&'static str],
    pub notes: &'static str,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostRegistration {
    pub backend_id: String,
    #[serde(default)]
    pub command: PathBuf,
    #[serde(default)]
    pub tcp_endpoint: Option<std::net::SocketAddr>,
    #[serde(default)]
    pub args: Vec<String>,
    pub trust: bool,
    #[serde(default)]
    pub allow_source_execution: bool,
    #[serde(default)]
    pub passive_profile: Option<crate::lsp::PassiveProfile>,
    #[serde(default)]
    pub initialization_options: Value,
    #[serde(default)]
    pub settings: Value,
    #[serde(default)]
    pub workspace_folders: Vec<String>,
    #[serde(default)]
    pub language_ids: BTreeMap<String, String>,
    #[serde(default)]
    pub open_languages: Vec<String>,
}
const ENTRIES: &[Entry] = &[
    Entry {
        id: "python",
        provider: "pyright",
        languages: &["python"],
        extensions: &["py", "pyi"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "python_jedi",
        provider: "jedi-language-server",
        languages: &["python"],
        extensions: &["py", "pyi"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "python_ty",
        provider: "ty",
        languages: &["python"],
        extensions: &["py", "pyi"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "python_pyrefly",
        provider: "pyrefly",
        languages: &["python"],
        extensions: &["py", "pyi"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "python_basedpyright",
        provider: "basedpyright",
        languages: &["python"],
        extensions: &["py", "pyi"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "java",
        provider: "Eclipse JDT LS",
        languages: &["java"],
        extensions: &["java"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "kotlin",
        provider: "Kotlin LSP",
        languages: &["kotlin"],
        extensions: &["kt", "kts"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "rust",
        provider: "rust-analyzer",
        languages: &["rust"],
        extensions: &["rs"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "csharp",
        provider: "Roslyn language server",
        languages: &["csharp"],
        extensions: &["cs"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "csharp_omnisharp",
        provider: "OmniSharp",
        languages: &["csharp"],
        extensions: &["cs"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "typescript",
        provider: "typescript-language-server",
        languages: &["typescript", "javascript", "tsx", "jsx"],
        extensions: &[
            "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "mtsx", "ctsx", "mjsx", "cjsx",
        ],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "typescript_vts",
        provider: "vtsls",
        languages: &["typescript", "javascript", "tsx", "jsx"],
        extensions: &[
            "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "mtsx", "ctsx", "mjsx", "cjsx",
        ],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "vue",
        provider: "Volar",
        languages: &["vue", "typescript", "javascript", "tsx", "jsx"],
        extensions: &[
            "vue", "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "mtsx", "ctsx", "mjsx",
            "cjsx",
        ],
        companions: &["typescript"],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "svelte",
        provider: "svelte-language-server",
        languages: &["svelte", "typescript", "javascript", "tsx", "jsx"],
        extensions: &[
            "svelte", "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "mtsx", "ctsx", "mjsx",
            "cjsx",
        ],
        companions: &["typescript"],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "astro",
        provider: "Astro language server",
        languages: &["astro", "typescript", "javascript", "tsx", "jsx"],
        extensions: &[
            "astro", "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "mtsx", "ctsx", "mjsx",
            "cjsx",
        ],
        companions: &["typescript"],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "go",
        provider: "gopls",
        languages: &["go"],
        extensions: &["go"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "ruby",
        provider: "ruby-lsp",
        languages: &["ruby"],
        extensions: &["rb", "erb"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "ruby_solargraph",
        provider: "solargraph",
        languages: &["ruby"],
        extensions: &["rb"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "dart",
        provider: "Dart analysis server",
        languages: &["dart"],
        extensions: &["dart"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "cpp",
        provider: "clangd",
        languages: &["cpp", "c"],
        extensions: &[
            "c", "h", "c++", "cc", "cp", "cpp", "cxx", "hh", "hpp", "hxx", "inl", "ipp", "tpp",
            "txx", "m", "mm", "c++m", "cppm", "cxxm", "ixx", "cu", "hip", "cl", "clcpp", "ino",
        ],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "cpp_ccls",
        provider: "ccls",
        languages: &["cpp", "c"],
        extensions: &[
            "c", "h", "c++", "cc", "cp", "cpp", "cxx", "hh", "hpp", "hxx", "inl", "ipp", "tpp",
            "txx", "m", "mm", "ino",
        ],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "php",
        provider: "Intelephense",
        languages: &["php"],
        extensions: &["php", "phtml"],
        companions: &[],
        notes: "Intelephense premium features require the provider license; optional keys are not acquired or stored here.",
    },
    Entry {
        id: "php_phpactor",
        provider: "Phpactor",
        languages: &["php"],
        extensions: &["php", "phtml"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "php_phpantom",
        provider: "PHPantom",
        languages: &["php"],
        extensions: &["php", "phtml"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "php_devsense",
        provider: "Devsense PHP LS",
        languages: &["php"],
        extensions: &["php", "phtml"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "perl",
        provider: "PerlLanguageServer",
        languages: &["perl"],
        extensions: &["pl", "pm", "t"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "clojure",
        provider: "ClojureLSP",
        languages: &["clojure"],
        extensions: &["clj", "cljs", "cljc", "edn"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "elixir",
        provider: "Expert",
        languages: &["elixir"],
        extensions: &["ex", "exs"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "elm",
        provider: "ElmLanguageServer",
        languages: &["elm"],
        extensions: &["elm"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "terraform",
        provider: "terraform-ls",
        languages: &["terraform"],
        extensions: &["tf", "tfvars", "tfstate"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "swift",
        provider: "SourceKit-LSP",
        languages: &["swift"],
        extensions: &["swift"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "bash",
        provider: "BashLanguageServer",
        languages: &["bash"],
        extensions: &["sh", "bash"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "crystal",
        provider: "CrystalLanguageServer",
        languages: &["crystal"],
        extensions: &["cr"],
        companions: &[],
        notes: "Crystalline has limited definitions and no reference search.",
    },
    Entry {
        id: "cue",
        provider: "CueLanguageServer",
        languages: &["cue"],
        extensions: &["cue"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "yaml",
        provider: "yaml-language-server",
        languages: &["yaml"],
        extensions: &["yaml", "yml"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "json",
        provider: "vscode-json-languageserver",
        languages: &["json"],
        extensions: &["json", "jsonc"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "toml",
        provider: "Taplo",
        languages: &["toml"],
        extensions: &["toml"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "zig",
        provider: "ZigLanguageServer",
        languages: &["zig"],
        extensions: &["zig", "zon"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "nix",
        provider: "nixd",
        languages: &["nix"],
        extensions: &["nix"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "lua",
        provider: "LuaLS",
        languages: &["lua"],
        extensions: &["lua"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "lua_emmylua",
        provider: "EmmyLua",
        languages: &["lua"],
        extensions: &["lua"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "luau",
        provider: "LuauLanguageServer",
        languages: &["luau"],
        extensions: &["luau"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "erlang",
        provider: "Erlang Language Platform",
        languages: &["erlang"],
        extensions: &["erl", "hrl", "escript", "config", "app", "app.src"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "ocaml",
        provider: "OcamlLanguageServer",
        languages: &["ocaml"],
        extensions: &["ml", "mli", "re", "rei"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "al",
        provider: "ALLanguageServer",
        languages: &["al"],
        extensions: &["al", "dal"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "rego",
        provider: "Regal",
        languages: &["rego"],
        extensions: &["rego"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "markdown",
        provider: "marksman",
        languages: &["markdown"],
        extensions: &["md", "markdown"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "latex",
        provider: "texlab",
        languages: &["latex"],
        extensions: &["tex", "bib", "sty", "cls"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "r",
        provider: "RLanguageServer",
        languages: &["r"],
        extensions: &["R", "r", "Rmd", "Rnw"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "scala",
        provider: "Metals",
        languages: &["scala"],
        extensions: &["scala", "sbt"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "julia",
        provider: "LanguageServer.jl",
        languages: &["julia"],
        extensions: &["jl"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "julia_fatou",
        provider: "Fatou",
        languages: &["julia"],
        extensions: &["jl"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "fortran",
        provider: "fortls",
        languages: &["fortran"],
        extensions: &["f90", "f95", "f03", "f08", "f", "for", "fpp"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "haskell",
        provider: "HaskellLanguageServer",
        languages: &["haskell"],
        extensions: &["hs", "lhs"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "haxe",
        provider: "HaxeLanguageServer",
        languages: &["haxe"],
        extensions: &["hx"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "lean4",
        provider: "Lean built-in LSP",
        languages: &["lean4"],
        extensions: &["lean"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "fsharp",
        provider: "FSharpLanguageServer",
        languages: &["fsharp"],
        extensions: &["fs", "fsx", "fsi"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "powershell",
        provider: "PowerShell Editor Services",
        languages: &["powershell"],
        extensions: &["ps1", "psm1", "psd1"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "pascal",
        provider: "PascalLanguageServer",
        languages: &["pascal"],
        extensions: &["pas", "pp", "lpr", "dpr", "dpk", "inc"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "groovy",
        provider: "GroovyLanguageServer",
        languages: &["groovy"],
        extensions: &["groovy", "gvy"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "matlab",
        provider: "MATLAB language server",
        languages: &["matlab"],
        extensions: &["m", "mlx", "mlapp"],
        companions: &[],
        notes: "Requires a licensed host MATLAB installation and compatible language server.",
    },
    Entry {
        id: "hlsl",
        provider: "shader-language-server",
        languages: &["hlsl"],
        extensions: &[
            "hlsl", "hlsli", "fx", "fxh", "cginc", "compute", "shader", "glsl", "vert", "frag",
            "geom", "tesc", "tese", "comp", "wgsl",
        ],
        companions: &[],
        notes: "Shader provider covers HLSL/GLSL/WGSL but does not support reference search.",
    },
    Entry {
        id: "systemverilog",
        provider: "SystemVerilogLanguageServer",
        languages: &["systemverilog"],
        extensions: &["sv", "svh", "v", "vh"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "solidity",
        provider: "SolidityLanguageServer",
        languages: &["solidity"],
        extensions: &["sol"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "ansible",
        provider: "AnsibleLanguageServer",
        languages: &["ansible"],
        extensions: &["yaml", "yml"],
        companions: &[],
        notes: "Upstream 1.2.3 lacks symbols, references and rename; negotiated unsupported operations remain unavailable.",
    },
    Entry {
        id: "msl",
        provider: "Host mSL language server",
        languages: &["msl"],
        extensions: &["mrc"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "bsl",
        provider: "BSL Language Server",
        languages: &["bsl"],
        extensions: &["bsl", "os"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "ada",
        provider: "Ada Language Server",
        languages: &["ada"],
        extensions: &["ads", "adb", "ada"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "gdscript",
        provider: "Godot built-in LSP",
        languages: &["gdscript"],
        extensions: &["gd", "gdscript"],
        companions: &[],
        notes: "Requires an explicitly selected running Godot TCP endpoint; stdio registration is unavailable.",
    },
    Entry {
        id: "qml",
        provider: "qmlls",
        languages: &["qml"],
        extensions: &["qml"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "gleam",
        provider: "Gleam built-in LSP",
        languages: &["gleam"],
        extensions: &["gleam"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "nextflow",
        provider: "NextflowLanguageServer",
        languages: &["nextflow"],
        extensions: &["nf"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "wolfram",
        provider: "Wolfram LSP",
        languages: &["wolfram"],
        extensions: &["wl", "wls"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "html",
        provider: "vscode-html-languageserver",
        languages: &["html"],
        extensions: &["html", "htm"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "scss",
        provider: "some-sass-language-server",
        languages: &["scss"],
        extensions: &["scss", "sass", "css"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "angular",
        provider: "Angular language server",
        languages: &["angular", "typescript", "javascript", "tsx", "jsx"],
        extensions: &["html", "htm", "ts", "tsx", "mts", "cts", "mtsx", "ctsx"],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
    Entry {
        id: "deno",
        provider: "Deno LSP",
        languages: &["typescript", "javascript", "tsx", "jsx"],
        extensions: &[
            "ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs", "mtsx", "ctsx", "mjsx", "cjsx",
        ],
        companions: &[],
        notes: "Catalog metadata grants no runtime authority. Select a measured passive host profile or explicit source execution permission; capabilities are negotiated at runtime.",
    },
];
pub fn entries() -> &'static [Entry] {
    ENTRIES
}
pub fn get(id: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.id == id)
}
impl Entry {
    pub fn supports_path(&self, path: &std::path::Path) -> bool {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        let name = name.to_ascii_lowercase();
        self.extensions
            .iter()
            .any(|extension| name.ends_with(&format!(".{}", extension.to_ascii_lowercase())))
    }
}
impl HostRegistration {
    pub fn configured(&self) -> Result<crate::lsp::ConfiguredServer> {
        let entry = get(&self.backend_id).context("Unknown language-server backend")?;
        ensure!(
            self.trust,
            "Language-server execution requires explicit host trust"
        );
        ensure!(
            entry.id != "gdscript" || self.tcp_endpoint.is_some(),
            "Godot requires an explicitly selected existing TCP endpoint"
        );
        let command = if let Some(endpoint) = self.tcp_endpoint {
            ensure!(
                endpoint.ip().is_loopback() && endpoint.port() != 0 && self.args.is_empty(),
                "LSP endpoint requires numeric loopback and no process arguments"
            );
            format!("tcp://{endpoint}")
        } else {
            ensure!(
                self.command.is_absolute() && self.command.is_file(),
                "Host server command must be an absolute existing executable path"
            );
            self.command
                .to_str()
                .context("Non-UTF-8 host server path")?
                .to_owned()
        };
        ensure!(
            command.len() <= 4096
                && !command.contains(['\0', '\r', '\n'])
                && self.args.len() <= 100
                && self
                    .args
                    .iter()
                    .all(|arg| arg.len() <= 64000 && !arg.contains('\0'))
                && self.args.iter().map(String::len).sum::<usize>() <= 128000,
            "Host command argument budget exceeded"
        );
        ensure!(
            serde_json::to_vec(&self.initialization_options)?.len() <= 64000
                && serde_json::to_vec(&self.settings)?.len() <= 64000
                && self.workspace_folders.len() <= 32
                && self.language_ids.len() <= 100
                && self.open_languages.len() <= 100,
            "Host configuration capacity exceeded"
        );
        for folder in &self.workspace_folders {
            if folder != "." {
                crate::paths::normalized(folder)?;
            }
        }
        ensure!(
            self.language_ids.iter().all(|(key, value)| !key.is_empty()
                && key.len() <= 100
                && !value.is_empty()
                && value.len() <= 100
                && !key.chars().chain(value.chars()).any(char::is_control))
                && self
                    .open_languages
                    .iter()
                    .all(|language| !language.is_empty()
                        && language.len() <= 100
                        && !language.chars().any(char::is_control)),
            "Invalid host language mapping"
        );
        let open_languages = if self.open_languages.is_empty() {
            entry
                .languages
                .iter()
                .map(|language| (*language).to_owned())
                .collect()
        } else {
            self.open_languages.clone()
        };
        Ok(crate::lsp::ConfiguredServer {
            server: crate::lsp::Server {
                command,
                args: self.args.clone(),
                language: entry.languages[0].into(),
                trust: true,
            },
            configuration: crate::lsp::HostConfiguration {
                passive_profile: self.passive_profile,
                allow_source_execution: self.allow_source_execution,
                refactor_intent_profile: None,
                tcp_endpoint: self.tcp_endpoint,
                initialization_options: self.initialization_options.clone(),
                settings: self.settings.clone(),
                workspace_folders: self.workspace_folders.clone(),
                language_ids: self.language_ids.clone(),
                open_languages,
            },
        })
    }
}
