from pathlib import Path
p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach\native\src\mixed_view_query.rs')
s=p.read_text(encoding='utf-8')
start=s.index('            let item = match source {',s.index('pub(super) fn run('))
end=s.index('            check(d)?;',start)
body=s[start:end].replace('            let item = match source {','    let item = match source {',1)
# Rustfmt will normalize the extracted indentation.
helper='''pub(super) fn read_item<'a>(
    view: &'a CheckedMixedView,
    source: &'a SourceRef,
    d: &Deadline,
) -> Result<Item<'a>> {
    let side = view.source.before();
    let snapshot = side.primary_snapshot();
    let scope = side.bind_scope(snapshot, d)?;
'''+body+'''    check(d)?;
    Ok(item)
}
'''
s=s[:start]+'            let item = read_item(view, source, d)?;\n'+s[end:]
s=s.replace('pub(super) fn run(',helper+'pub(super) fn run(',1)
s=s.replace("struct Item<'a> {","pub(super) struct Item<'a> {",1).replace("    text: &'a str,\n    xml_origins:","    pub(super) text: &'a str,\n    xml_origins:",1)
s=s.replace("struct Metadata<'a> {","pub(super) struct Metadata<'a> {",1).replace("fn metadata<'a>(","pub(super) fn metadata<'a>(",1)
p.write_text(s,encoding='utf-8',newline='\n')
