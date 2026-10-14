#!/usr/bin/env python3
"""Package the reviewed local runtime as a VSIX without registry access.

This validates local package inputs and ZIP structure, not extension-host behavior.
"""
from __future__ import annotations

import argparse
import hashlib
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native-archive',type=Path)
    parser.add_argument('--include-typescript',action='store_true')
    parser.add_argument('--output', type=Path,
                        default=ROOT / f"code-intelligence-{package['version']}.vsix")
    args=parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        raise SystemExit('Archive exists; choose a new output path.')
    native_files:dict[str,bytes]={}
    target_platform=None
    if args.native_archive:
        with zipfile.ZipFile(args.native_archive) as native:
            info=json.loads(native.read('manifest.json'))
            if info['version']!=package['version']:
                raise SystemExit('Extension/native runtime versions differ.')
            targets={'x86_64-pc-windows-msvc':('win32-x64','yoke.exe'),'x86_64-unknown-linux-gnu':('linux-x64','yoke'),'aarch64-apple-darwin':('darwin-arm64','yoke'),'x86_64-apple-darwin':('darwin-x64','yoke')}
            if info['target'] not in targets:
                raise SystemExit('Unsupported native VSIX target.')
            target_platform,binary_name=targets[info['target']]
            binary=native.read(binary_name)
            if hashlib.sha256(binary).hexdigest()!=info['binarySha256']:
                raise SystemExit('Native binary checksum mismatch.')
            built=ROOT/'target/release'/binary_name
            if built.is_file() and hashlib.sha256(built.read_bytes()).hexdigest()!=info['binarySha256']:
                raise SystemExit('Native archive does not match the current release executable.')
            for entry in native.infolist():
                parts=Path(entry.filename).parts
                if entry.is_dir():
                    continue
                if entry.file_size>128_000_000 or entry.filename.startswith(('/', '\\')) or '\\' in entry.filename or '..' in parts or (entry.external_attr>>16)&0o170000==0o120000:
                    raise SystemExit('Unsafe native archive entry.')
                native_files['extension/runtime/native/'+entry.filename]=native.read(entry)
    elif package['version']!=runtime['version']:
        raise SystemExit('Supply the matching --native-archive for this extension version.')
    include_typescript=args.include_typescript or not args.native_archive
    required_inputs=['LICENSE']+(['dist/cli.js','node_modules/typescript/lib/typescript.js'] if include_typescript else [])
    for required in required_inputs:
        if not (ROOT / required).is_file():
            raise SystemExit(f'Missing build input: {required}')
    files: dict[str, Path] = {}
    for directory, prefix in [(ROOT / 'extensions/vscode', 'extension'),
                              *([(ROOT / d, 'extension/runtime/' + d)
                                for d in ['dist', 'workers', 'node_modules/typescript']] if include_typescript else [])]:
        for source in sorted(directory.rglob('*')):
            relative = source.relative_to(directory)
            if (source.is_file() and not source.is_symlink()
                    and '__pycache__' not in relative.parts
                    and source.name != 'test-host.cjs'
                    and source.suffix != '.pyc'):
                files[prefix + '/' + relative.as_posix()] = source
    files['extension/LICENSE'] = ROOT / 'LICENSE'
    if include_typescript:
        files['extension/runtime/package.json'] = ROOT / 'package.json'
        files['extension/runtime/THIRD_PARTY_NOTICES.md'] = ROOT / 'THIRD_PARTY_NOTICES.md'
    manifest = ET.Element('PackageManifest', Version='2.0.0', xmlns=VSX)
    metadata = ET.SubElement(manifest, 'Metadata')
    identity=ET.SubElement(metadata, 'Identity', Language='en-US', Id=package['name'],
                  Version=package['version'], Publisher=package['publisher'])
    if target_platform:
        identity.set('TargetPlatform',target_platform)
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
    extensions = {Path(name).suffix.lstrip('.') for name in [*files,*native_files] if Path(name).suffix}
    extensions.add('vsixmanifest')
    for ext in sorted(extensions):
        ET.SubElement(content_types, 'Default', Extension=ext,
                      ContentType=types.get(ext, 'application/octet-stream'))
    for name in [*files,*native_files]:
        if not Path(name).suffix:
            ET.SubElement(content_types, 'Override', PartName='/' + name,
                          ContentType='text/plain')
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, 'x', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        archive.writestr('extension.vsixmanifest', ET.tostring(manifest, encoding='utf-8', xml_declaration=True))
        archive.writestr('[Content_Types].xml', ET.tostring(content_types, encoding='utf-8', xml_declaration=True))
        for name, source in files.items():
            archive.write(source, name)
        for name,data in native_files.items():
            entry=zipfile.ZipInfo(name,date_time=(2026,10,14,0,0,0))
            entry.create_system=3
            entry.compress_type=zipfile.ZIP_DEFLATED
            entry.external_attr=(0o100755 if name=='extension/runtime/native/yoke' else 0o100644)<<16
            archive.writestr(entry,data)
    with zipfile.ZipFile(output) as archive:
        if archive.testzip() is not None:
            raise SystemExit('ZIP integrity validation failed')
    print(str(output))


if __name__ == '__main__':
    main()
