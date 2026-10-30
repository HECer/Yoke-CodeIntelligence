# Native DOCX and XLSX inspection

`yoke --root <project> media inspect <relative.docx-or-xlsx>` reads an explicitly
selected asset through the existing source path, ignore, link and secret policy.
It captures stable original bytes and retains their SHA-256 identity. Office is
not started, package members are not written to disk, external relationships are
not followed, and formulas are not recalculated. No model is called.

Word inspection reads the main body, tables, and internally related headers,
footers, footnotes, endnotes and comments. It joins text runs within each
paragraph, retaining tabs and explicit breaks. Spreadsheet inspection follows
workbook relationships in sheet order, resolves shared/rich and inline strings,
and returns explicitly addressed cell values. Formula expressions remain separate
from their cached values; missing cached values remain explicitly unavailable.
Phonetic annotation text is excluded from shared string values.

Each segment records its package part, SHA-256 of the decompressed XML, and exact
UTF-8 XML byte span. Its `text` is decoded content, not an assertion that the same
bytes occur literally in XML. Container identity and part identity are separate.
These are document observations, not rendered layout, OCR, Excel calculations,
compiler facts or model interpretation. Raw cached numbers, dates and errors are
not formatted through Excel styles. Shared formula expressions are not expanded.

The ZIP directory is admitted before library directory allocation: at most 2,048
members, a 2 MB directory, 16 MB per expanded member and 32 MB expanded overall.
Stored and deflate members are supported. Multidisk/ZIP64 directories, encrypted
or special members, traversing names, case aliases, overlapping spans, inconsistent
local names and CRC failures are rejected. Nothing is silently extracted with a
sanitized replacement name.
Decoded names must equal their admitted UTF-8 directory names, including after
Unicode extra-field processing; extra fields cannot change package identity.

XML is UTF-8/XML 1.0 only. A namespace-free admission pass bounds tag size,
namespace declarations, attributes, depth and text before namespace registration
or DOM text copies. The resolved parser rejects duplicate expanded attributes,
unknown entities, document types and forbidden XML scalars. Main content types,
relationship roles and document roots must agree. Obvious secrets are checked in
package content and decoded fields before persistence.

Decoded text, including returned formula expressions, is limited to 2 MB; shared
strings have a separate 2 MB table bound. At most 20,000 segments are admitted.
Serialized segment bytes are counted incrementally with a 7.9 MB limit before
appending a row; the complete immutable media artifact has the existing 8 MB
bound. Capacity errors are explicit. Large successful CLI responses respect
`--max-bytes` and return an immutable artifact ID instead of dropping content.

The implementation uses pinned [zip 9.0.3](https://docs.rs/zip/9.0.3/zip/) and
[quick-xml 0.42.0](https://docs.rs/quick-xml/0.42.0/quick_xml/). Format references are
Microsoft's [WordprocessingML overview](https://learn.microsoft.com/en-us/office/open-xml/word/overview)
and [shared string table documentation](https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/working-with-the-shared-string-table).

This adds native media inspection and immutable artifact transport. Publishing
these extracted documents into a derived searchable graph still requires its
separate typed source-origin integration; inspection does not silently add them
to the code snapshot.
