# Yoke Code Intelligence — Rust 0.5.0

**Lokale Codeanalyse mit exakten Quellbelegen, unveränderlichen Projektständen und überprüfbaren Änderungen.**

[English](README.md) · [Rust-Anleitung](native/README.md) · [Schnellstart](docs/QUICKSTART.md) · [Funktionsstand](docs/STATUS.md) · [Ersatzmatrix](docs/RUST_REPLACEMENT.md)

Die eigenständige Rust-Lösung verbindet Codesuche, gerichtete Beziehungen, Sprachserver-Navigation und kontrollierte Änderungen. Rust steuert außerdem konfigurierte Compiler, Testauswahl, Modellanbieter mit Kostenjournal, persistentes Python-Debugging, signierten TLS-Quellaustausch, Historienbereinigung und Docker-Verifikation. Die Standardfunktionen benötigen weder Graft, Serena oder Graphify noch Node, Python, ein Sprachmodell oder einen API-Schlüssel. Die TypeScript-Version 0.3 bleibt mit getrennten Zuständen verfügbar.

## Start

Zum Bauen: Rust 1.94 oder neuer und die Plattform-Werkzeuge. Auf Windows wurde die MSVC-Toolchain verwendet.

```sh
cargo build --workspace --release --locked -j 2
./target/release/yoke --root /pfad/zum/projekt index
./target/release/yoke --root /pfad/zum/projekt find meineFunktion
./target/release/yoke --root /pfad/zum/projekt context meineFunktion
./target/release/yoke --root /pfad/zum/projekt export --format html
./target/release/yoke --root /pfad/zum/projekt serve
```

Auf Windows heißt das Programm `target/release/yoke.exe`. Das portable Archiv enthält das Programm und vollständige Lizenzhinweise. Projektdaten und private Schlüssel werden nicht gebündelt. Rust speichert Zustände in `.yoke`, getrennt vom TypeScript-Verzeichnis `.code-intelligence`.

## Native Funktionen

| Bereich | Umsetzung |
|---|---|
| Quellen | Strenge Erfassung, SHA-256-Blobs, exakte UTF-8-Bereiche, Ausschlussregeln und wiederverwendete Parsergebnisse |
| Analyse | Tree-sitter-Struktur für 24 Sprachen, gerichtete Mehrfachbeziehungen, Service-Verträge und sichtbare Unsicherheiten |
| Suche | FTS/BM25, Text/Regex, Quellkontext, begrenzte Abfragepläne, verankerte Erkenntnisse und statistische/externe Vektoren |
| Semantik | Eigener LSP-Client; ausdrücklich gewählte Sprachserver; Definitionen, Referenzen und vorbereitete Umbenennungen |
| Änderungen | Überprüfbare Pläne, isolierte Anwendung, ausgeführte Verifikation, Freigaben und dauerhafte Wiederherstellung |
| Integration | Acht MCP-Werkzeuge, CLI, Offline-Graphansicht, versionierte Editorpuffer und authentifizierte lokale HTTP-Schnittstelle |
| Medien | Technische Bild-/Audio-/Videometadaten, gelieferte Transkripte und tatsächliche PDF-Textextraktion im begrenzten Worker |
| Austausch | Signierte unveränderliche Quellen, ausdrückliche Empfangsfreigaben, getrennte Remote-Zustände und lokale Neuindexierung |
| Wartung | Exklusive Client-Sperren, geschützte Historie/Artefakte und wiederherstellbare Quarantäne unreferenzierter Blobs |

Die VS-Code-Erweiterung verwendet standardmäßig Rust und akzeptiert ausführbare Programme ausschließlich aus Benutzereinstellungen. Ungespeicherte Änderungen bleiben von gespeicherten Quelldateien getrennt. Private Signaturschlüssel liegen im Windows Credential Manager.

## Prüfungen und Grenzen

Windows mit Node 24 wurde tatsächlich getestet. Die TypeScript-Suite besteht mit **240 erfolgreichen und 3 übersprungenen Tests**. Echte rust-analyzer-/TypeScript-Sprachserverläufe, native Leiden- und PDF-Prüfungen, die Offline-Graphansicht im Browser und ein tatsächlicher VS-Code-1.140-Erweiterungsprozess liefern zusätzliche Nachweise. Protokollfixtures bleiben separat gekennzeichnet.

Die [native Prüfliste](docs/validation/native-workflows.json) und der [Funktionsstand](docs/STATUS.md) unterscheiden echte Prüfungen und Protokollfixtures. Konfigurierte TypeScript-Kompilierung, Python-Debugger/REPL, Docker und Windows-Zugangsdaten wurden tatsächlich ausgeführt. TLS-Transport, Modellprotokolle und konservative Historienbereinigung haben eigene Verhaltenstests. Gehostete Linux/macOS-CI, sämtliche Sprachserver und proprietäre Cloud-Produkte benötigen zusätzliche Nachweise. Quarantäne erhält Bytes; ausschließlich eine exakt bestätigte Bereinigung darf unreferenzierte Quarantänedaten löschen. Ein Mikrobenchmark beweist keine Tokenersparnis ganzer Agentenaufgaben.

Für den erweiterten TypeScript-Pfad gelten `npm run build`, `npm run typecheck` und `npm test`; Details stehen in [ADVANCED.md](docs/ADVANCED.md). Änderungen an Originalquellen bleiben standardmäßig gesperrt. Eigener Code ist MIT; Serena-Anwendungscode wurde nicht übernommen. Eine lokale Paketierung führt keine GitHub- oder Registry-Veröffentlichung aus.
