#!/usr/bin/env python3
"""Optional real-browser test. Requires Playwright and an installed Chromium.
Run after npm run build. It never connects to a public service.
"""
import argparse
import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from datetime import datetime, timezone
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--chromium', default=shutil.which('chromium'))
    parser.add_argument('--save', action='store_true')
    parser.add_argument('--offline-fixture', action='store_true', help='Test actual frontend against in-page fixture responses, without browser networking')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='ci-browser-') as temporary:
        fixture = Path(temporary)
        (fixture / 'src').mkdir()
        (fixture / 'src' / 'invoice.ts').write_text(
            'export interface InvoiceLine { quantity: number; unitPrice: number }\n\n'
            '/** Sum line totals before applying the configured tax rate. */\n'
            'export function calculateInvoice(lines: InvoiceLine[], taxRate = 0.19): number {\n'
            '  const subtotal = lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0);\n'
            '  return Math.round(subtotal * (1 + taxRate) * 100) / 100;\n}\n', encoding='utf-8')
        (fixture / 'src' / 'checkout.ts').write_text(
            "import { calculateInvoice } from './invoice.js';\n"
            'export function checkout() { return calculateInvoice([{quantity: 2, unitPrice: 19.9}]); }\n', encoding='utf-8')
        (fixture / 'README.md').write_text('# Checkout\nInvoice and checkout demonstration.\n', encoding='utf-8')
        subprocess.run(['git', 'init', '-q', str(fixture)], check=True)
        process = subprocess.Popen(['node', str(ROOT / 'dist' / 'cli.js'), 'view', '--root', str(fixture), '--timeout', '120000'],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8')
        try:
            # URL is a capability secret: never write it to the report or screenshot.
            buffer = ''
            for _ in range(32):
                line = process.stdout.readline()
                if not line:
                    raise RuntimeError('Viewer did not start: ' + process.stderr.read())
                buffer += line
                try:
                    startup = json.loads(buffer)
                    break
                except json.JSONDecodeError:
                    continue
            else:
                raise RuntimeError('Invalid viewer startup payload')
            url = startup['url']
            errors = []
            with sync_playwright() as playwright:
                launch = {'headless': True, 'args': ['--no-sandbox']}
                if args.chromium:
                    launch['executable_path'] = args.chromium
                browser = playwright.chromium.launch(**launch)
                page = browser.new_page(viewport={'width': 1440, 'height': 1050}, device_scale_factor=1)
                page.on('pageerror', lambda error: errors.append(str(error)))
                if args.offline_fixture:
                    # No proxying or bypassing the managed browser network policy.
                    # This is an explicitly mocked DOM test, not a live API test.
                    html = (ROOT / 'workers' / 'viewer.html').read_text(encoding='utf-8')
                    html = html.replace('<link rel="stylesheet" href="/viewer.css">', '').replace('<script src="/viewer.js" defer></script>', '')
                    page.set_content(html)
                    page.add_style_tag(content=(ROOT / 'workers' / 'viewer.css').read_text(encoding='utf-8'))
                    fixture_source = (fixture / 'src' / 'invoice.ts').read_text(encoding='utf-8')
                    page.evaluate("""source => {
                      location.hash = 'offline-fixture-not-an-access-token';
                      // about:blank has no same-origin URL for replaceState.
                      history.replaceState = () => { location.hash = ''; };
                      window.fetch = async (_url, options) => {
                        const req = JSON.parse(options.body), id = 'a'.repeat(64), snapshotId = 'b'.repeat(64);
                        const entity = {id, name:'calculateInvoice', qualifiedName:'calculateInvoice', path:'src/invoice.ts', kind:'function', startLine:4};
                        let data = {coverage:{indexedFiles:3,unresolved:0}};
                        if(req.tool==='ci_find') data={items:[entity]};
                        if(req.tool==='ci_read') data={text:source};
                        if(req.tool==='ci_trace') data={observations:[{source:id,target:'c'.repeat(64),relation:'calls',resolution:'exact_binding',evidenceIds:['d'.repeat(64)]}]};
                        return {ok:true,json:async()=>({schemaVersion:1,snapshotId,status:'ok',data,warnings:[]})};
                      };
                    }""", fixture_source)
                    page.add_script_tag(content=(ROOT / 'workers' / 'viewer.js').read_text(encoding='utf-8'))
                else:
                    page.goto(url)
                page.locator('#index').click()
                page.wait_for_function("document.querySelector('#coverage').textContent.includes('indexed files')")
                page.locator('#query').fill('calculateInvoice')
                page.locator('#search button[type=submit]').click()
                page.locator('.result').first.wait_for()
                page.locator('.result').first.click()
                page.wait_for_function("document.querySelector('#detail').textContent.includes('Math.round')")
                assert page.evaluate('location.hash') == '', 'Capability token was not removed from the browser URL'
                assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'Desktop horizontal overflow'
                if args.save:
                    out = ROOT / 'docs' / 'screenshots'; out.mkdir(parents=True, exist_ok=True)
                    page.screenshot(path=str(out / 'explorer.png'), full_page=True)
                page.locator('#trace').click()
                page.wait_for_function("document.querySelector('#detail').textContent.includes('evidenceIds')")
                page.set_viewport_size({'width': 390, 'height': 844})
                assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'Mobile horizontal overflow'
                assert page.locator('#query').is_visible(), 'Search not available on mobile'
                assert not errors, errors
                version = browser.version
                browser.close()
            report = {'passed': True, 'measuredAt': datetime.now(timezone.utc).isoformat(), 'browser': 'Chromium', 'version': version,
                      'checks': ['index control and status rendering', 'symbol search control/results', 'source rendering', 'trace control/evidence rendering',
                                 'fragment cleanup requested', 'desktop no horizontal overflow', 'mobile search and no horizontal overflow', 'no page exceptions'],
                      'network': 'none: mocked frontend API' if args.offline_fixture else 'loopback only',
                      'mode': 'offline_dom_fixture' if args.offline_fixture else 'live_loopback_integration',
                      'limitations': ['Live browser-to-loopback navigation was blocked by the managed Chromium policy; independent HTTP backend tests are separate.', 'Offline mode mocks fetch responses and the about:blank history API; it does not validate live browser networking, CSP execution or token handling.'] if args.offline_fixture else [],
                      'fixture': 'generated non-user invoice demo', 'errors': errors}
            if args.save:
                out = ROOT / 'docs' / 'validation'; out.mkdir(parents=True, exist_ok=True)
                (out / 'browser.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
            print(json.dumps(report, indent=2))
        finally:
            process.terminate()
            try:
                process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill(); process.communicate()

if __name__ == '__main__':
    main()
