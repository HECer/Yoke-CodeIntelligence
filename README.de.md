# Code Intelligence

**Lokale Codeanalyse mit Quellbelegen, unveränderlichen Projektständen und überprüfbaren Änderungen.**

[English](README.md) · [Schnellstart](docs/QUICKSTART.md) · [Funktionsstand](docs/STATUS.md) · [Prüfergebnisse](docs/validation/report.json)

Die Implementierung verbindet Codesuche, einen gerichteten Wissensgraphen, semantische TypeScript-Navigation und einen abgesicherten Änderungsablauf. Für die eingebauten Analysepfade ist weder ein Sprachmodell noch ein API-Schlüssel erforderlich.

## Start

Voraussetzungen: Node.js ab 22.16 und Git; für Python-Dateien Python ab 3.10, für Go-Dateien eine Go-Toolchain.

```sh
npm ci
npm run build
node dist/cli.js index --root "C:/Projekte/MeinProjekt" --timeout 120000
node dist/cli.js find meineFunktion --root "C:/Projekte/MeinProjekt"
node dist/cli.js view --root "C:/Projekte/MeinProjekt" --timeout 120000
```

Das separate Runtime-Archiv enthält bereits die gebaute Anwendung und TypeScript. Damit kann `node dist/cli.js --help` direkt ohne Paketinstallation gestartet werden. Node, Python, Go und optionale Upstream-Tools sind nicht darin enthalten.

Für eine Agentenanbindung wird `serve` statt `view` gestartet. Die acht MCP-Werkzeuge und konkrete JSON-Beispiele stehen in [docs/API.md](docs/API.md). Die üblichen Schritte sind: finden, Originalcode lesen, Änderung vorbereiten, prüfen und zunächst isoliert anwenden. Änderungen am Originalprojekt sind standardmäßig gesperrt.

## Was tatsächlich enthalten ist

Unveränderliche Snapshots und Quell-Hashes, UTF-8-Positionen, SQLite-Volltextsuche, gerichtete Beziehungen mit Herkunft und Unsicherheiten, TypeScript-/JavaScript-Symbolsuche und Umbenennungen, Python-AST- und Go-Parseranalyse, begrenzte Kontextpläne, Konfliktschutz und Wiederherstellung unterbrochener Änderungen. Hinzu kommen ein lokaler Explorer, Gedächtniseinträge mit Quellenbindung und Importe von Vektoren, Laufzeitbeobachtungen, LCOV und SARIF.

Graft und Graphify besitzen versionierte Import-/Prozessadapter. Externe LSP- und Serena-MCP-Anbindungen sind ebenfalls implementiert. **Diese Adapter wurden mit Vertrags- und Protokollfixtures geprüft, nicht mit tatsächlich installierten aktuellen Upstream-Paketen.** Version 0.1.0 ist deshalb kein Versprechen vollständiger Funktionsgleichheit oder Produktionsreife. Die verbleibenden Unterschiede sind in [STATUS.md](docs/STATUS.md) ausdrücklich aufgelistet.

Die lokale Testsuite und die Mikrobenchmark-Ergebnisse sind beigefügt. Der Screenshot zeigt die echte Oberfläche mit einer Testantwort: Direkte Browseraufrufe auf localhost wurden durch eine Richtlinie dieser Umgebung blockiert. Der HTTP-Dienst wurde unabhängig davon getestet.

## Öffentlich auf GitHub veröffentlichen

Ziel: **`HECer/Code-Intelligence`**. Das Repository konnte in dieser Sitzung nicht erstellt werden: Der verbundene GitHub-Connector bietet keine Repository-Anlage; ein Schreibversuch auf das noch nicht vorhandene Ziel lieferte 404.

Im entpackten Quellprojekt mit enthaltener `.git`-Historie:

```sh
gh auth login
npm run publish:github
```

Dafür ist GitHub CLI nötig. Alternativ zu npm: `node scripts/publish-github.mjs`. Der Publisher prüft den Account `HECer`, legt das Repository öffentlich an, pusht ohne Force und verifiziert Sichtbarkeit und Commit. Er überschreibt kein abweichendes Repository und fordert keine im Chat geteilten Zugangsdaten.

## Wichtige Grenzen

Gespeicherte Dateien, keine ungespeicherten Editorbuffer. Python-/Go-Aufrufziele sind häufig heuristisch, nicht compilerverifiziert. Ein statischer Graph ist kein Laufzeit- oder Sicherheitsbeweis. Kopierte Arbeitsverzeichnisse schützen nicht gegen bösartige ausgeführte Programme; externe Verifikationsbefehle benötigen ausdrückliches Vertrauen. Ein kleiner Warm-Query-Benchmark beweist keine Tokenersparnis eines kompletten Agentenlaufs.

Der eigene Code steht unter MIT. Die separat installierten Upstream-Komponenten behalten ihre jeweiligen Lizenzen; Serena-Anwendungscode wurde nicht übernommen.
