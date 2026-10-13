#!/usr/bin/env python3
"""Package the reviewed local runtime as a VSIX without registry access.

This validates local package inputs and ZIP structure, not extension-host behavior.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent.parent
VSX = 'http://schemas.microsoft.com/developer/vsx-schema/2011'
CT = 'http://schemas.openxmlformats.org/package/2006/content-types'


def main() -> None:
    package = json.loads((ROOT / 'extensions/vscode/package.json').read_text())
    runtime = json.loads((ROOT / 'package.json').read_text())
    if package['version'] != runtime['version']:
        raise SystemExit('Extension/runtime versions differ; build the intended release first.')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path,
                        default=ROOT / f"code-intelligence-{package['version']}.vsix")
    output = parser.parse_args().output.resolve()
    for required in ['dist/cli.js', 'node_modules/typescript/lib/typescript.js', 'LICENSE']:
        if not (ROOT / required).is_file():
            raise SystemExit(f'Missing build input: {required}')
    files: dict[str, Path] = {}
    for directory, prefix in [(ROOT / 'extensions/vscode', 'extension'),
                              *[(ROOT / d, 'extension/runtime/' + d)
                                for d in ['dist', 'workers', 'node_modules/typescript']]]:
        for source in sorted(directory.rglob('*')):
            relative = source.relative_to(directory)
            if (source.is_file() and not source.is_symlink()
                    and '__pycache__' not in relative.parts
                    and source.suffix != '.pyc'):
                files[prefix + '/' + relative.as_posix()] = source
    files['extension/LICENSE'] = ROOT / 'LICENSE'
    files['extension/runtime/package.json'] = ROOT / 'package.json'
    files['extension/runtime/THIRD_PARTY_NOTICES.md'] = ROOT / 'THIRD_PARTY_NOTICES.md'
    manifest = ET.Element('PackageManifest', Version='2.0.0', xmlns=VSX)
    metadata = ET.SubElement(manifest, 'Metadata')
    ET.SubElement(metadata, 'Identity', Language='en-US', Id=package['name'],
                  Version=package['version'], Publisher=package['publisher'])
    ET.SubElement(metadata, 'DisplayName').text = package['displayName']
    ET.SubElement(metadata, 'Description').text = package['description']
    ET.SubElement(metadata, 'Tags').text = 'code,intelligence'
    ET.SubElement(metadata, 'Categories').text = 'Other'
    properties = ET.SubElement(metadata, 'Properties')
    ET.SubElement(properties, 'Property', Id='Microsoft.VisualStudio.Code.Engine',
                  Value=package['engines']['vscode'])
    ET.SubElement(metadata, 'License').text = 'extension/LICENSE'
    installation = ET.SubElement(manifest, 'Installation')
    ET.SubElement(installation, 'InstallationTarget', Id='Microsoft.VisualStudio.Code')
    ET.SubElement(manifest, 'Dependencies')
    assets = ET.SubElement(manifest, 'Assets')
    ET.SubElement(assets, 'Asset', Type='Microsoft.VisualStudio.Code.Manifest',
                  Path='extension/package.json', Addressable='true')
    content_types = ET.Element('Types', xmlns=CT)
    types = {'json': 'application/json', 'vsixmanifest': 'text/xml',
             'js': 'application/javascript', 'cjs': 'application/javascript',
             'mjs': 'application/javascript', 'md': 'text/markdown',
             'html': 'text/html', 'css': 'text/css', 'png': 'image/png',
             'py': 'text/plain', 'go': 'text/plain', 'ts': 'text/plain',
             'txt': 'text/plain'}
    extensions = {Path(name).suffix.lstrip('.') for name in files if Path(name).suffix}
    extensions.add('vsixmanifest')
    for ext in sorted(extensions):
        ET.SubElement(content_types, 'Default', Extension=ext,
                      ContentType=types.get(ext, 'application/octet-stream'))
    for name in files:
        if not Path(name).suffix:
            ET.SubElement(content_types, 'Override', PartName='/' + name,
                          ContentType='text/plain')
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        archive.writestr('extension.vsixmanifest', ET.tostring(manifest, encoding='utf-8', xml_declaration=True))
        archive.writestr('[Content_Types].xml', ET.tostring(content_types, encoding='utf-8', xml_declaration=True))
        for name, source in files.items():
            archive.write(source, name)
    with zipfile.ZipFile(output) as archive:
        if archive.testzip() is not None:
            raise SystemExit('ZIP integrity validation failed')
    print(str(output))


if __name__ == '__main__':
    main()
