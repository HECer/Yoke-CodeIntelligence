#!/usr/bin/env python3
"""Package the reviewed local runtime as a VSIX without registry access.

This validates local package inputs and ZIP structure, not extension-host behavior.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent.parent
VSX = 'http://schemas.microsoft.com/developer/vsx-schema/2011'
CT = 'http://schemas.openxmlformats.org/package/2006/content-types'
NATIVE_ENTRY_BYTES = 128_000_000
NATIVE_TOTAL_BYTES = 256_000_000
NATIVE_ARCHIVE_BYTES = 256_000_000
NATIVE_MANIFEST_BYTES = 1_000_000
NATIVE_ENTRIES = 8_192
ASCII_LOWER = str.maketrans('ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz')


def native_zip_directory(source):
    # Check the bounded EOCD before ZipFile allocates the central-directory list.
    source.seek(0, 2)
    size = source.tell()
    if size > NATIVE_ARCHIVE_BYTES:
        raise ValueError('Native archive byte budget exceeded')
    start = max(0, size - 65_557)
    source.seek(start)
    tail = source.read(65_557)
    position = len(tail)
    while True:
        position = tail.rfind(b'PK\x05\x06', 0, position)
        if position < 0:
            raise ValueError('Native ZIP directory unavailable')
        if position + 22 <= len(tail):
            end = struct.unpack_from('<4s4H2LH', tail, position)
            if position + 22 + end[-1] == len(tail):
                break
    _, disk, directory_disk, disk_entries, entries, directory_bytes, offset, _ = end
    if (disk or directory_disk or disk_entries != entries or entries > NATIVE_ENTRIES
            or directory_bytes > 8_000_000 or offset + directory_bytes != start + position):
        raise ValueError('Unsupported or over-budget native ZIP directory')
    source.seek(0)
    return entries


def native_member_key(entry):
    name = entry.filename
    if name != entry.orig_filename or '\\' in name or len(name) > 4096:
        raise ValueError('Unsafe native archive name')
    parts = (name[:-1] if entry.is_dir() else name).split('/')
    for part in parts:
        device = part.split('.', 1)[0].translate(ASCII_LOWER)
        if (not part or part in ('.', '..') or part[-1] in ' .'
                or any(ord(char) < 32 or ord(char) == 127 or char in ':*?"<>|' for char in part)
                or len(part.encode('utf-16-le')) > 510
                or device in {'con', 'prn', 'aux', 'nul', 'conin$', 'conout$'}
                or re.fullmatch(r'(?:com|lpt)[1-9\u00b9\u00b2\u00b3]', device)):
            raise ValueError('Unsafe native archive name')
    mode = (entry.external_attr >> 16) & 0o170000
    if mode not in (0, 0o040000 if entry.is_dir() else 0o100000) or entry.flag_bits & 1:
        raise ValueError('Linked, encrypted or special native archive entry')
    return '/'.join(parts).translate(ASCII_LOWER)


def native_archive(path, version):
    """Admit all entry metadata before inflating manifest or executable bytes."""
    targets = {
        'x86_64-pc-windows-msvc': ('win32-x64', 'yoke.exe'),
        'x86_64-unknown-linux-gnu': ('linux-x64', 'yoke'),
        'aarch64-apple-darwin': ('darwin-arm64', 'yoke'),
        'x86_64-apple-darwin': ('darwin-x64', 'yoke'),
    }

    def unique_object(pairs):
        value = {}
        for key, item in pairs:
            if key in value:
                raise ValueError('Duplicate native manifest field')
            value[key] = item
        return value

    def reject_constant(_):
        raise ValueError('Non-JSON native manifest constant')

    try:
        if path.is_symlink() or not path.is_file() or path.stat().st_size > NATIVE_ARCHIVE_BYTES:
            raise ValueError('Native archive input unavailable or exceeds byte budget')
        with path.open('rb') as source:
            entries = native_zip_directory(source)
            return read_native_zip(source, entries, version, targets, unique_object, reject_constant)
    except (OSError, ValueError, RuntimeError, RecursionError, zipfile.BadZipFile,
            NotImplementedError, struct.error) as error:
        raise SystemExit('Native archive validation failed.') from error


def read_native_zip(source, expected_entries, version, targets, unique_object, reject_constant):
    with zipfile.ZipFile(source) as archive:
        entries = archive.infolist()
        if len(entries) != expected_entries or len(entries) > NATIVE_ENTRIES:
            raise ValueError('Native archive entry budget exceeded')
        admitted = {}
        total = 0
        for entry in entries:
            key = native_member_key(entry)
            if key in admitted:
                raise ValueError('Duplicate or aliased native archive entry')
            admitted[key] = entry
            total += entry.file_size
            if (entry.file_size > NATIVE_ENTRY_BYTES
                    or total > NATIVE_TOTAL_BYTES
                    or entry.is_dir() and entry.file_size):
                raise ValueError('Native archive payload budget exceeded')
        for key in admitted:
            parts = key.split('/')
            for index in range(1, len(parts)):
                parent = admitted.get('/'.join(parts[:index]))
                if parent is not None and not parent.is_dir():
                    raise ValueError('Native archive file/directory conflict')
        manifest = admitted.get('manifest.json')
        if (manifest is None or manifest.filename != 'manifest.json' or manifest.is_dir()
                or manifest.file_size > NATIVE_MANIFEST_BYTES):
            raise ValueError('Native manifest unavailable or exceeds byte budget')
        info = json.loads(archive.read(manifest).decode('utf-8'),
                          object_pairs_hook=unique_object, parse_constant=reject_constant)
        if (not isinstance(info, dict) or info.get('version') != version
                or not isinstance(info.get('target'), str) or info['target'] not in targets
                or not isinstance(info.get('binarySha256'), str)
                or not re.fullmatch('[0-9a-f]{64}', info['binarySha256'])):
            raise ValueError('Invalid native manifest version, target or checksum')
        target, binary_name = targets[info['target']]
        binary_entry = admitted.get(binary_name)
        if (binary_entry is None or binary_entry.filename != binary_name
                or binary_entry.is_dir()):
            raise ValueError('Native executable unavailable')
        files = {entry.filename: archive.read(entry)
                 for entry in entries if not entry.is_dir()}
        if any(len(files[entry.filename]) != entry.file_size
               for entry in entries if not entry.is_dir()):
            raise ValueError('Native archive entry length mismatch')
        if hashlib.sha256(files[binary_name]).hexdigest() != info['binarySha256']:
            raise ValueError('Native binary checksum mismatch')
        built = ROOT / 'target/release' / binary_name
        if built.is_file():
            if (built.stat().st_size != len(files[binary_name])
                    or hashlib.sha256(built.read_bytes()).hexdigest() != info['binarySha256']):
                raise ValueError('Native archive differs from current release executable')
        return target, {'extension/runtime/native/' + name: data for name, data in files.items()}


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
        target_platform,native_files=native_archive(args.native_archive, package['version'])
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
