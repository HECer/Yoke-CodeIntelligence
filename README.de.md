# Code Intelligence 0.3.0

**Lokale Codeanalyse mit Quellbelegen, unveränderlichen Projektständen und überprüfbaren Änderungen.**

[English](README.md) · [Schnellstart](docs/QUICKSTART.md) · [Neue Funktionen](docs/ADVANCED.md) · [Funktionsstand](docs/STATUS.md) · [Prüfergebnisse](docs/validation/report.json)

Der eigenständige Kern verbindet Codesuche, gerichtete Beziehungen, konfigurationsgebundene TypeScript-Analyse und kontrollierte Änderungen. Native Indexierung benötigt kein Sprachmodell und keinen API-Schlüssel. Optionale externe Anbieter behalten eigene Installation, Berechtigungen und Lizenzen. Das ist **keine behauptete vollständige Funktionsgleichheit mit sämtlichen Upstream- oder Cloud-Produkten**.

## Start

Node.js **22.16 oder neuer**, Git; Python und Go für die entsprechenden optionalen Analysepfade. Das Runtime-Archiv enthält die gebaute Anwendung samt TypeScript und benötigt keine npm-Installation. Node/Python/Go, zusätzliche Python-Pakete und Modelle sind nicht gebündelt.

```sh
node dist/cli.js --help
node dist/cli.js index --root "C:/Projekte/MeinProjekt" --timeout 120000
node dist/cli.js find meineFunktion --root "C:/Projekte/MeinProjekt"
node dist/cli.js view --root "C:/Projekte/MeinProjekt" --timeout 120000
```

Im Quellarchiv zuerst `npm ci` und `npm run build`. In dieser Bauumgebung scheiterte die Online-Installation an DNS; Build und Tests nutzten die vorhandenen gepinnten Abhängigkeiten. Die acht MCP-Werkzeuge stehen über `serve` bereit. Änderungen am Originalprojekt bleiben standardmäßig gesperrt.

## Neu gegenüber 0.2.0

| Bereich | Implementierung |
|---|---|
| TypeScript | Unveränderliche, schreibgeschützte installierte Typdefinitionen; Solution-Build einschließlich Project-Reference-Ausgaben ausschließlich im Speicher. |
| Python/Go/LSP | Echte Jedi- und Go-Typanalyse für Navigation und vorbereitete Umbenennungen. Live-Prüfungen mit Jedi, Go und clangd. |
| Editor | Ungespeicherte Buffer mit Versions-/Konfliktschutz, lokale authentifizierte HTTP-Brücke und VS-Code-Erweiterung als VSIX. |
| Mehrere Projekte/Rechner | Signierter Snapshot-Austausch, projekt-/mandantenbezogene Berechtigungen und föderierte Suche ohne automatisches Überschreiben des Quellprojekts. |
| Laufende Pflege | Metadatenmodus, kontextbezogene Semantik-Caches, Watch-Reconciliation, explizite Speicherbereinigung und optionale periodische Retention. |
| Wissensanreicherung | Lokale statistische Vektoren, optionale neuronale Embeddings, Reranking und quellengebundene Modell-Zusammenfassungen. |
| Analyse | Zusätzliche Prisma-/SQL-/API-Bezüge, OpenAPI-Änderungsanalyse, begrenzter Datenfluss und Community-Backends. |
| Tests/Medien | Tatsächlich ausgeführte Node-Testselektionen mit Coverage, native PDF-/Metadaten-/Untertitelimporte; keine erfundenen Transkripte. |
| Prüfwerkzeuge | Docker-Profil mit explizitem Image-Digest und gesperrtem Netzwerk; getrennte Live-Gates und ein gepaarter Agenten-Benchmark-Harness. |

## Nachweise und offene Grenzen

Die vollständige Standardsuite wurde erneut ausgeführt. Zusätzliche echte Live-Tests und die Paketprüfungen sind im [Report](docs/validation/report.json) separat aufgeführt. Modell-/Serena-Protokollfixtures werden nicht als echte Anbieterläufe gezählt. Die Mikrobenchmarks sind keine nachgewiesene Tokenersparnis ganzer Agentenaufgaben.

Die konkreten Upstream-Pakete Graft/Graphify/Serena konnten hier nicht live installiert werden. Leiden, neuronale Modellgewichte, Docker und ein echter VS-Code-Host waren ebenfalls nicht verfügbar. Windows/macOS- und Hosted-CI-Läufe sind vorbereitet, nicht durchgeführt. Der Browserzugriff auf localhost wurde durch eine Administratorrichtlinie blockiert; Frontend- und echte HTTP-Tests sind deshalb getrennt ausgewiesen. Eine unabhängige Sicherheitsprüfung oder ein realer Agentenvergleich wurde nicht ersetzt oder erfunden.

## Weitere Verwendung

Die Anleitung [ADVANCED.md](docs/ADVANCED.md) enthält alle neuen Befehle. Die VSIX lässt sich lokal installieren; sie benötigt einen vertrauenswürdig konfigurierten Node-Prozess. Föderation und Schlüsselaustausch stehen in [FEDERATION.md](docs/FEDERATION.md). Vollständige Funktionsgrenzen: [STATUS.md](docs/STATUS.md).

**In diesem Update keine GitHub-Veröffentlichung.** Das vorhandene Veröffentlichungsskript bleibt optional im Projekt; es wurde nicht ausgeführt. Quellarchiv und Git-Historie sind die ausgelieferte Grundlage.

Eigener Code: MIT. Separat installierte Programme und Modelle haben eigene Lizenzbedingungen. Serena-Anwendungscode wurde nicht übernommen. Ein kopiertes Arbeitsverzeichnis ist keine Betriebssystem-Sandbox; externe Programme dürfen nur mit ausdrücklichem Vertrauen oder geeigneter Host-Isolation laufen.
