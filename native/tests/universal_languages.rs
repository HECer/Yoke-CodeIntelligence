use yoke::{
    analysis,
    capture::{Deadline, language},
    model::{SourceFile, hash},
};
fn extract(path: &str, language: &str, text: &str) -> yoke::model::Extraction {
    analysis::extract(
        &SourceFile {
            path: path.into(),
            language: language.into(),
            hash: hash(text.as_bytes()),
            size: text.len(),
            mode: 0o644,
        },
        text,
        &Deadline::new(10000),
    )
    .unwrap()
}
#[test]
fn file_metadata_does_not_copy_configuration_values_into_graph_signatures() {
    for (path, language, text) in [
        ("app.yaml", "yaml", "TOKEN: PRIVATE_RUNTIME_VALUE\n"),
        ("app.toml", "toml", "token = 'PRIVATE_RUNTIME_VALUE'\n"),
        (
            "app.json",
            "json",
            "{\"token\":\"PRIVATE_RUNTIME_VALUE\"}\n",
        ),
    ] {
        let result = extract(path, language, text);
        let file = result
            .entities
            .iter()
            .find(|entity| entity.kind == "file")
            .unwrap();
        assert!(
            !file.signature.contains("PRIVATE_RUNTIME_VALUE"),
            "{path}: {}",
            file.signature
        );
        assert_eq!(file.start_byte, 0);
        assert_eq!(file.end_byte, text.len());
    }
}
#[test]
fn newly_supported_extensions_are_not_silently_plain_text() {
    for (path, want) in [
        ("a.ps1", "powershell"),
        ("a.jl", "julia"),
        ("a.m", "objc"),
        ("a.f90", "fortran"),
        ("a.erl", "erlang"),
        ("a.lisp", "commonlisp"),
        ("a.pas", "pascal"),
        ("a.cob", "cobol"),
        ("a.vb", "vbnet"),
        ("a.sv", "verilog"),
        ("a.tf", "hcl"),
        ("a.cls", "apex"),
        ("a.dm", "dreammaker"),
        ("a.robot", "robot"),
        ("a.vue", "vue"),
        ("a.svelte", "svelte"),
        ("a.astro", "astro"),
        ("a.hxx", "cpp"),
        ("a.hh", "cpp"),
        ("a.bb", "cpp"),
    ] {
        assert_eq!(language(path), want, "{path}");
    }
}
#[test]
fn added_grammars_extract_declarations_from_real_source() {
    for (lang, source, want) in [
        (
            "powershell",
            "function Invoke-Sample { param($x) return $x }",
            "Invoke-Sample",
        ),
        ("julia", "function sample(x)\n return x\nend\n", "sample"),
        ("objc", "@interface Sample\n- (void)run;\n@end\n", "Sample"),
        (
            "fortran",
            "subroutine sample(x)\ninteger :: x\nend subroutine sample\n",
            "sample",
        ),
        ("erlang", "sample(X) -> X.\n", "sample"),
        ("commonlisp", "(defun sample (x) x)", "sample"),
        (
            "pascal",
            "program Test; procedure sample; begin end; begin end.",
            "sample",
        ),
        (
            "vbnet",
            "Module Test\n Sub sample()\n End Sub\nEnd Module\n",
            "sample",
        ),
        (
            "verilog",
            "module sample(input wire x); endmodule",
            "sample",
        ),
        (
            "hcl",
            "resource \"aws_instance\" \"sample\" { ami = \"abc\" }",
            "sample",
        ),
        (
            "apex",
            "public class Sample { public void run() {} }",
            "Sample",
        ),
    ] {
        let result = extract("a.source", lang, source);
        assert!(
            result
                .entities
                .iter()
                .any(|e| e.name == want && e.kind != "file"),
            "{lang}: {:?}",
            result
                .entities
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
        for e in result.entities {
            assert!(source.is_char_boundary(e.start_byte) && source.is_char_boundary(e.end_byte));
        }
    }
}
#[test]
fn vue_scripts_keep_original_utf8_offsets_and_ignore_markup_calls() {
    let text = "<template>🌍 misleading()</template>\n<script lang=\"ts\">export function first(){second();}</script>\n<script setup lang=\"ts\">function second(){}</script>";
    let result = extract("a.vue", "vue", text);
    for name in ["first", "second"] {
        let entity = result.entities.iter().find(|e| e.name == name).unwrap();
        assert_eq!(
            &text[entity.name_start_byte.unwrap()..entity.name_end_byte.unwrap()],
            name
        );
    }
    assert!(
        result
            .edges
            .iter()
            .any(|e| e.relation == "calls" && e.target_label == "second")
    );
    assert!(!result.edges.iter().any(|e| e.target_label == "misleading"));
}
#[test]
fn structural_fallbacks_extract_named_declarations_with_exact_ranges() {
    for (lang, source, want) in [
        (
            "cobol",
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SAMPLE.\n       PROCEDURE DIVISION.\n       RUN-SAMPLE.\n           DISPLAY 'hello'.\n",
            "SAMPLE",
        ),
        (
            "dreammaker",
            "/obj/sample\n    proc/run()\n        return 1\n",
            "run",
        ),
        (
            "robot",
            "*** Keywords ***\nSample Keyword\n    Log    hello\n",
            "Sample Keyword",
        ),
    ] {
        let result = extract("a.source", lang, source);
        let symbol = result
            .entities
            .iter()
            .find(|e| e.name == want)
            .unwrap_or_else(|| panic!("{lang}: {:?}", result.entities));
        assert_eq!(
            &source[symbol.name_start_byte.unwrap()..symbol.name_end_byte.unwrap()],
            want
        );
        assert_eq!(symbol.producer, "yoke-structural-patterns-1");
    }
}
#[test]
fn composite_scripts_skip_html_comments_and_non_executable_data() {
    let text = "<!-- <script>function wrong(){}</script> -->\n<script type=\"application/json\">{\"text\":\"function wrongAgain(){}\"}</script>\n<script>function actual(){}</script>";
    let result = extract("a.vue", "vue", text);
    assert!(result.entities.iter().any(|e| e.name == "actual"));
    assert!(
        !result
            .entities
            .iter()
            .any(|e| e.name == "wrong" || e.name == "wrongAgain")
    );
}
#[test]
fn astro_frontmatter_svelte_scripts_and_ejs_blocks_are_source_bound() {
    for (language, text, name) in [
        (
            "astro",
            "---\nfunction front(){return 1;}\n---\n<div>🌍</div>",
            "front",
        ),
        (
            "svelte",
            "<p>🌍</p><script lang=\"ts\">function script(){}</script>",
            "script",
        ),
        (
            "ejs",
            "<p>🌍</p><% function render(){ return 1; } %>",
            "render",
        ),
    ] {
        let result = extract("a.template", language, text);
        let symbol = result.entities.iter().find(|e| e.name == name).unwrap();
        assert_eq!(
            &text[symbol.name_start_byte.unwrap()..symbol.name_end_byte.unwrap()],
            name
        );
    }
}
#[test]
fn language_specific_calls_keep_the_call_expression_anchor() {
    for (language, text, name) in [
        (
            "powershell",
            "function Main { Invoke-Sample 1 }",
            "Invoke-Sample",
        ),
        ("julia", "function main()\n sample(1)\nend\n", "sample"),
        ("erlang", "main() -> sample(1).", "sample"),
        ("pascal", "program Test; begin sample(1); end.", "sample"),
        (
            "fortran",
            "subroutine main\ncall sample(1)\nend subroutine main\n",
            "sample",
        ),
        ("hcl", "locals { result = sample(1) }", "sample"),
    ] {
        let result = extract("a.source", language, text);
        let call = result
            .edges
            .iter()
            .find(|e| e.relation == "calls" && e.target_label == name)
            .unwrap_or_else(|| panic!("{language}: {:?}", result.edges));
        assert!(text[call.start_byte..call.end_byte].contains(name));
        assert!(text.is_char_boundary(call.start_byte) && text.is_char_boundary(call.end_byte));
    }
}
#[test]
fn r_object_system_declarations_preserve_literal_names_and_methods() {
    let source = r#"setClass("Animal", slots=list(name="character"))
setGeneric("speak", function(x) standardGeneric("speak"))
setMethod("speak", "Animal", function(x) x@name)
Pet <- R6::R6Class("Pet", public=list(run=function() 1))
Owner <- S7::new_class("Owner", properties=list(name=class_character))
print.Animal <- function(x) x
"#;
    let result = extract("a.R", "r", source);
    for (name, kind) in [
        ("Animal", "class"),
        ("speak", "method"),
        ("Pet", "class"),
        ("Owner", "class"),
        ("print.Animal", "function"),
    ] {
        let symbol = result
            .entities
            .iter()
            .find(|e| e.name == name && e.kind == kind)
            .unwrap_or_else(|| {
                panic!(
                    "{name}/{kind}: {:?}",
                    result
                        .entities
                        .iter()
                        .map(|e| (&e.name, &e.kind))
                        .collect::<Vec<_>>()
                )
            });
        assert_eq!(
            &source[symbol.name_start_byte.unwrap()..symbol.name_end_byte.unwrap()],
            name
        );
    }
}
#[test]
fn blade_php_and_razor_code_blocks_keep_template_byte_anchors() {
    for (language, text, name) in [
        (
            "blade",
            "<p>🌍</p>\n@php\nfunction render() { return 1; }\n@endphp\n",
            "render",
        ),
        (
            "razor",
            "<p>🌍</p>\n@code {\n void Render() { Run(); }\n}\n",
            "Render",
        ),
    ] {
        let result = extract("a.template", language, text);
        let symbol = result
            .entities
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{language}: {:?}", result.entities));
        assert_eq!(
            &text[symbol.name_start_byte.unwrap()..symbol.name_end_byte.unwrap()],
            name
        );
    }
}
#[test]
fn r6_named_methods_and_s7_method_assignments_are_distinct_declarations() {
    let text = "Pet <- R6::R6Class(\"Pet\", public=list(run=function() 1))\nmethod(speak, Pet) <- function(x) x\n";
    let result = extract("a.R", "r", text);
    for name in ["run", "speak"] {
        let method = result
            .entities
            .iter()
            .find(|e| e.name == name && e.kind == "method")
            .unwrap_or_else(|| panic!("{name}: {:?}", result.entities));
        assert_eq!(
            &text[method.name_start_byte.unwrap()..method.name_end_byte.unwrap()],
            name
        );
    }
}
#[test]
fn dreammaker_call_lines_cannot_be_mislabeled_as_function_declarations() {
    let text = "/obj/sample\n    proc/run()\n        helper()\n        return 1\n";
    let result = extract("a.dm", "dreammaker", text);
    assert!(result.entities.iter().any(|e| e.name == "run"));
    assert!(!result.entities.iter().any(|e| e.name == "helper"));
}
#[test]
fn large_robot_keyword_sets_respect_the_shared_deadline_without_quadratic_section_scans() {
    let mut text = "*** Keywords ***\n".to_string();
    for i in 0..20000 {
        text.push_str(&format!("Keyword {i}\n    Log    hello\n"));
    }
    let file = SourceFile {
        path: "a.robot".into(),
        language: "robot".into(),
        hash: hash(text.as_bytes()),
        size: text.len(),
        mode: 0o644,
    };
    let extraction = analysis::extract(&file, &text, &Deadline::new(1000)).unwrap();
    assert_eq!(
        extraction
            .entities
            .iter()
            .filter(|e| e.kind == "function")
            .count(),
        20000
    );
}
#[test]
fn object_and_module_declarations_keep_named_method_owners() {
    let mut missing = Vec::new();
    for (lang, text, want) in [
        (
            "objc",
            "@implementation Sample\n- (void)run { }\n@end\n",
            "run",
        ),
        ("powershell", "class Sample { [void] Run() { } }", "Run"),
        (
            "fortran",
            "module Sample\ncontains\nsubroutine run()\nend subroutine run\nend module Sample\n",
            "Sample",
        ),
        (
            "verilog",
            "module Sample; function integer run; run=1; endfunction endmodule",
            "run",
        ),
        ("commonlisp", "(defclass Sample () ())", "Sample"),
        (
            "pascal",
            "unit Test; interface type Sample = class end; implementation end.",
            "Sample",
        ),
    ] {
        let result = extract("a.source", lang, text);
        let Some(symbol) = result.entities.iter().find(|e| e.name == want) else {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&analysis::grammar(lang).unwrap())
                .unwrap();
            eprintln!(
                "{lang}: {}",
                parser.parse(text, None).unwrap().root_node().to_sexp()
            );
            missing.push(format!(
                "{lang}: {:?}",
                result
                    .entities
                    .iter()
                    .map(|e| (&e.name, &e.kind))
                    .collect::<Vec<_>>()
            ));
            continue;
        };
        assert_eq!(
            &text[symbol.name_start_byte.unwrap()..symbol.name_end_byte.unwrap()],
            want
        );
    }
    assert!(missing.is_empty(), "Missing declarations: {missing:?}");
}
#[test]
fn ejs_comment_and_escaped_tags_do_not_become_code() {
    let text = "<%# function fakeComment(){ fakeCall(); } %><%% function fakeLiteral(){ fakeCall(); } %><% function actual(){} %>";
    let result = extract("a.ejs", "ejs", text);
    assert!(result.entities.iter().any(|e| e.name == "actual"));
    assert!(!result.entities.iter().any(|e| e.name.starts_with("fake")));
    assert!(!result.edges.iter().any(|e| e.target_label == "fakeCall"));
}
#[test]
fn separate_vue_scripts_cannot_repair_each_others_syntax_or_span_markup() {
    let text = "<script>function mixed(){</script><p>markup</p><script>call();}</script>";
    let result = extract("a.vue", "vue", text);
    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .entities
            .iter()
            .filter(|e| e.kind != "file")
            .all(|e| !text[e.start_byte..e.end_byte].contains("<p>markup</p>"))
    );
}
#[test]
fn unsupported_embedded_language_is_explicit_and_quoted_tag_attributes_are_respected() {
    let text = "<script lang=\"coffee\">function fakeCoffee(){fakeCall();}</script><script title=\">\" type=\"application/json\">function fakeData(){fakeCall();}</script><script>function actual(){}</script>";
    let result = extract("a.vue", "vue", text);
    assert!(result.entities.iter().any(|e| e.name == "actual"));
    assert!(!result.entities.iter().any(|e| e.name.starts_with("fake")));
    assert!(result.diagnostics.iter().any(|d|d.message.contains("Unsupported embedded language")&&d.severity=="warning"));
}
#[test]
fn harmless_html_attribute_ampersands_do_not_abort_script_extraction() {
    let text = "<script title=\"A&B\">function actual(){}</script>";
    let result = extract("a.vue", "vue", text);
    assert!(result.entities.iter().any(|e| e.name == "actual"));
}
#[test]
fn duplicate_script_types_keep_the_first_authored_attribute() {
    let text =
        "<script type=\"application/json\" type=\"text/javascript\">function fake(){}</script>";
    let result = extract("a.vue", "vue", text);
    assert!(!result.entities.iter().any(|e| e.name == "fake"));
}
#[test]
fn razor_raw_strings_and_interpolations_do_not_close_the_outer_code_block() {
    let text = r#"@code {
 void First() { var raw = """ a " } " b """; var interpolated = $"{Get("}")}"; }
 void Last() {}
}"#;
    let result = extract("a.razor", "razor", text);
    assert!(result.entities.iter().any(|e| e.name == "Last"));
}
