#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# ///
"""Analyze the data.gov inventory dump to understand what binoc should support.

Input is the `metadata.jsonl` produced by harvard-lil/gov-data
(https://source.coop/harvard-lil/gov-data): one JSON object per line, one line
per archived data.gov dataset. Each record carries the full data.gov (CKAN)
metadata for the dataset plus a listing of the files actually captured into the
dataset's nabit bag.

We care about two questions for binoc feature prioritization:

  1. What file *formats* show up across data.gov, and how many resources of each?
     `resources[].format` is the agency-declared format (often blank), so we
     fall back to the file extension parsed from `resources[].url`.

  2. What did the archive *actually capture*? `zip_entries` lists every file in
     each bag; the real fetched payloads live under `data/files/`. Their
     extensions are the ground truth for what binoc would diff between two
     snapshots, independent of what the catalog claimed.

Each format token is mapped to a "binoc bucket" aligned with binoc-stdlib's
correspondence rule families (tabular, structured_document, archive, text,
database, document, image, web_page, web_service, executable, other) so the
output reads directly as a feature-priority list.

The input is multi-GB; we stream it line by line and never hold more than the
running counters (plus size/count lists used for percentiles).

Usage:
    scripts/analyze_data_gov_inventory.py path/to/metadata.jsonl
    scripts/analyze_data_gov_inventory.py metadata.jsonl --limit 20000
    scripts/analyze_data_gov_inventory.py metadata.jsonl --sample-every 10
    scripts/analyze_data_gov_inventory.py metadata.jsonl --json report.json
"""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import sys

# --- format / extension -> binoc bucket -------------------------------------
#
# Keys are lowercased, dot-stripped tokens. We match against both the declared
# `format` (e.g. "CSV", "ESRI REST") and the URL file extension (e.g. "csv").
# Anything unmatched lands in "other" and is surfaced in the raw breakdowns so
# we don't silently bucket away a long tail worth looking at.

BUCKET_BY_TOKEN: dict[str, str] = {}


def _register(bucket: str, *tokens: str) -> None:
    for t in tokens:
        BUCKET_BY_TOKEN[t] = bucket


# tabular: row/column data — binoc-stdlib CSV rule, future xlsx/parquet
_register(
    'tabular', 'csv', 'tsv', 'tab', 'xls', 'xlsx', 'xlsm', 'xlsb', 'ods', 'dbf', 'parquet', 'psv'
)
# structured_document: tree-shaped text/binary scientific & geo data
_register(
    'structured_document',
    'json',
    'jsonld',
    'geojson',
    'xml',
    'rdf',
    'kml',
    'kmz',
    'gml',
    'yaml',
    'yml',
    'toml',
    'atom',
    'gpx',
    'svg',
    'nc',
    'netcdf',
    'cdf',
    'hdf',
    'hdf5',
    'h5',
    'grib',
    'grib2',
    'bufr',
)
# archive: containers binoc expands and recurses into
_register('archive', 'zip', 'tar', 'gz', 'tgz', 'gzip', 'bz2', 'tbz', '7z', 'rar', 'xz', 'zst', 'z')
# database: single-file databases
_register('database', 'sqlite', 'sqlite3', 'db', 'mdb', 'accdb', 'sql', 'gdb')
# text: plain prose / unstructured text
_register('text', 'txt', 'text', 'md', 'rst', 'log', 'readme', 'asc')
# document: rendered documents
_register('document', 'pdf', 'doc', 'docx', 'rtf', 'odt', 'ppt', 'pptx', 'epub')
# image / raster
_register(
    'image',
    'tif',
    'tiff',
    'geotiff',
    'jpg',
    'jpeg',
    'jp2',
    'png',
    'gif',
    'bmp',
    'sid',
    'mrsid',
    'webp',
    'ecw',
    'img',
)
# media
_register(
    'media', 'mp4', 'avi', 'wmv', 'mov', 'mp3', 'wav', 'mpg', 'mpeg', 'm4v', 'video/x-msvideo'
)
# web pages
_register('web_page', 'html', 'htm', 'xhtml', 'asp', 'aspx', 'php', 'cfm')
# live web services / APIs — not a downloadable file binoc diffs
_register(
    'web_service',
    'esri rest',
    'arcgis geoservices rest api',
    'wms',
    'wfs',
    'wcs',
    'sos',
    'ogc wms',
    'ogc wfs',
    'api',
    'rest',
    'esri rest service',
    'wmts',
    'ogc:wms',
    'ogc:wfs',
)
# executables / opaque binaries
_register('executable', 'exe', 'dll', 'bin', 'msi', 'dmg', 'jar')

# Buckets that represent things binoc can meaningfully diff as files.
FILE_BUCKETS = {
    'tabular',
    'structured_document',
    'archive',
    'database',
    'text',
    'document',
    'image',
    'media',
    'executable',
}

BUCKET_ORDER = [
    'tabular',
    'structured_document',
    'archive',
    'database',
    'text',
    'document',
    'image',
    'media',
    'web_page',
    'web_service',
    'executable',
    'other',
]

# bag scaffolding to ignore when looking at what was actually captured.
BAG_SCAFFOLDING_PREFIXES = ('signatures/',)
BAG_SCAFFOLDING_FILES = {
    'bagit.txt',
    'bag-info.txt',
    'manifest-sha256.txt',
    'tagmanifest-sha256.txt',
    'data/signed-metadata.json',
    'data/headers.warc',
}


def url_extension(url: str | None) -> str:
    """Lowercase file extension from a URL path, or '' if none looks real.

    Only the path is considered, never the host — otherwise bare-domain URLs
    like ``https://www.ncei.noaa.gov`` yield a spurious "gov" extension. Also
    handles plain filenames (e.g. zip_entries paths) that have no scheme/host.
    """
    if not url:
        return ''
    # strip query/fragment
    url = url.split('?', 1)[0].split('#', 1)[0]
    if '://' in url:
        # drop scheme://host; keep the path only. No "/" after host => no path.
        rest = url.split('://', 1)[1]
        if '/' not in rest:
            return ''
        path = rest.split('/', 1)[1].rstrip('/')
    else:
        path = url.rstrip('/')
    if not path:
        return ''
    seg = path.rsplit('/', 1)[-1]
    if '.' not in seg:
        return ''
    ext = seg.rsplit('.', 1)[-1].lower()
    # extensions are short and alphanumeric; reject anything that looks like a
    # domain segment, a number, or a long opaque slug.
    if not (1 <= len(ext) <= 7) or not ext.isalnum():
        return ''
    return ext


def normalize_format(fmt: str | None) -> str:
    return (fmt or '').strip().lower()


def classify(fmt: str | None, url: str | None) -> tuple[str, str]:
    """Return (binoc_bucket, token_used) for a resource.

    Prefer a recognizable declared format; fall back to the URL extension.
    """
    fmt_norm = normalize_format(fmt)
    if fmt_norm in BUCKET_BY_TOKEN:
        return BUCKET_BY_TOKEN[fmt_norm], fmt_norm
    ext = url_extension(url)
    if ext in BUCKET_BY_TOKEN:
        return BUCKET_BY_TOKEN[ext], ext
    # unrecognized but present: keep a token for the long-tail report
    token = fmt_norm or ext or '(none)'
    return 'other', token


def percentiles(values: list[int], ps=(50, 90, 99)) -> dict[str, int]:
    if not values:
        return {f'p{p}': 0 for p in ps}
    s = sorted(values)
    out = {}
    for p in ps:
        idx = min(len(s) - 1, max(0, round((p / 100) * (len(s) - 1))))
        out[f'p{p}'] = s[idx]
    return out


class Stats:
    def __init__(self) -> None:
        self.datasets = 0
        self.dataset_states: Counter[str] = Counter()
        self.organizations: Counter[str] = Counter()
        self.resources = 0
        self.declared_format: Counter[str] = Counter()
        self.url_ext: Counter[str] = Counter()
        self.bucket: Counter[str] = Counter()
        self.other_tokens: Counter[str] = Counter()
        self.mimetype: Counter[str] = Counter()
        self.resources_per_dataset: list[int] = []
        self.resource_sizes: list[int] = []
        self.total_size_bytes = 0
        # actually-captured payloads (data/files/*)
        self.captured_files = 0
        self.captured_ext: Counter[str] = Counter()
        self.captured_bucket: Counter[str] = Counter()
        self.bags_with_capture = 0
        self.bags_with_nested_archive = 0

    def add_record(self, d: dict) -> None:
        self.datasets += 1
        dgm = d.get('signed_metadata', {}).get('data_gov_metadata', {}) or {}
        self.dataset_states[dgm.get('state') or '(none)'] += 1
        org = (dgm.get('organization') or {}).get('name') or '(none)'
        self.organizations[org] += 1

        resources = dgm.get('resources') or []
        self.resources_per_dataset.append(len(resources))
        for r in resources:
            self.resources += 1
            fmt = r.get('format')
            url = r.get('url')
            self.declared_format[normalize_format(fmt) or '(none)'] += 1
            self.url_ext[url_extension(url) or '(none)'] += 1
            bucket, token = classify(fmt, url)
            self.bucket[bucket] += 1
            if bucket == 'other':
                self.other_tokens[token] += 1
            mt = (r.get('mimetype') or '(none)').strip().lower()
            self.mimetype[mt] += 1
            size = r.get('size')
            if isinstance(size, (int, float)) and size > 0:
                self.resource_sizes.append(int(size))
                self.total_size_bytes += int(size)

        # what the archive actually captured
        captured_here = 0
        nested_here = False
        for e in d.get('zip_entries') or []:
            fn = e.get('filename', '')
            if not fn.startswith('data/files/'):
                continue
            if fn in BAG_SCAFFOLDING_FILES or fn.endswith('/'):
                continue
            self.captured_files += 1
            captured_here += 1
            ext = url_extension(fn)
            self.captured_ext[ext or '(none)'] += 1
            bucket, _ = classify(None, fn)
            self.captured_bucket[bucket] += 1
            if bucket == 'archive':
                nested_here = True
        if captured_here:
            self.bags_with_capture += 1
        if nested_here:
            self.bags_with_nested_archive += 1

    # --- reporting ----------------------------------------------------------
    def to_dict(self, top: int) -> dict:
        rpd = self.resources_per_dataset
        file_resources = sum(self.bucket[b] for b in self.bucket if b in FILE_BUCKETS)
        return {
            'datasets': self.datasets,
            'dataset_states': dict(self.dataset_states.most_common()),
            'resources': self.resources,
            'file_resources': file_resources,
            'resources_per_dataset': {
                'mean': round(sum(rpd) / len(rpd), 2) if rpd else 0,
                **percentiles(rpd),
                'max': max(rpd) if rpd else 0,
            },
            'resource_size_bytes': {
                'count_with_size': len(self.resource_sizes),
                'total': self.total_size_bytes,
                **percentiles(self.resource_sizes),
                'max': max(self.resource_sizes) if self.resource_sizes else 0,
            },
            'binoc_bucket_by_resource': {
                b: self.bucket.get(b, 0) for b in BUCKET_ORDER if self.bucket.get(b, 0)
            },
            'declared_format_top': dict(self.declared_format.most_common(top)),
            'url_extension_top': dict(self.url_ext.most_common(top)),
            'other_tokens_top': dict(self.other_tokens.most_common(top)),
            'mimetype_top': dict(self.mimetype.most_common(top)),
            'organizations_top': dict(self.organizations.most_common(top)),
            'captured': {
                'bags_with_capture': self.bags_with_capture,
                'bags_with_nested_archive': self.bags_with_nested_archive,
                'files': self.captured_files,
                'bucket': {
                    b: self.captured_bucket.get(b, 0)
                    for b in BUCKET_ORDER
                    if self.captured_bucket.get(b, 0)
                },
                'extension_top': dict(self.captured_ext.most_common(top)),
            },
        }


def _bar(n: int, total: int, width: int = 28) -> str:
    if total <= 0:
        return ''
    filled = round(width * n / total)
    return '█' * filled + '·' * (width - filled)


def _print_counter(title: str, items: dict, total: int) -> None:
    print(f'\n{title}')
    for k, v in items.items():
        pct = (100 * v / total) if total else 0
        print(f'  {v:>9,}  {pct:5.1f}%  {_bar(v, total)}  {k}')


def print_report(stats: Stats, top: int) -> None:
    r = stats.to_dict(top)
    print('=' * 72)
    print('data.gov inventory analysis  (binoc feature-priority view)')
    print('=' * 72)
    print(f'\ndatasets:            {r["datasets"]:>12,}')
    print(f'resources (links):   {r["resources"]:>12,}')
    print(f'  of which files:    {r["file_resources"]:>12,}  (rest are web pages / live services)')

    rpd = r['resources_per_dataset']
    print(
        f'\nresources per dataset:  mean {rpd["mean"]}  '
        f'median {rpd["p50"]}  p90 {rpd["p90"]}  p99 {rpd["p99"]}  '
        f'max {rpd["max"]}'
    )

    sz = r['resource_size_bytes']
    print(
        f'declared sizes:  {sz["count_with_size"]:,} resources have a size  '
        f'(median {sz["p50"]:,}B  p90 {sz["p90"]:,}B  '
        f'p99 {sz["p99"]:,}B  max {sz["max"]:,}B)'
    )

    _print_counter(
        'binoc bucket — by declared resource (format or URL extension):',
        r['binoc_bucket_by_resource'],
        r['resources'],
    )

    cap = r['captured']
    if cap['files']:
        print('\n' + '-' * 72)
        print('WHAT THE ARCHIVE ACTUALLY CAPTURED (data/files/* in each bag)')
        print(
            f'bags with captured payload: {cap["bags_with_capture"]:,}   '
            f'bags containing a nested archive: '
            f'{cap["bags_with_nested_archive"]:,}'
        )
        _print_counter('captured payload by binoc bucket:', cap['bucket'], cap['files'])
        _print_counter('captured payload by file extension:', cap['extension_top'], cap['files'])

    _print_counter('declared format (raw, top):', r['declared_format_top'], r['resources'])
    _print_counter('URL file extension (raw, top):', r['url_extension_top'], r['resources'])
    _print_counter(
        "unrecognized 'other' tokens (worth a look):", r['other_tokens_top'], r['resources']
    )
    _print_counter('top organizations (by dataset):', r['organizations_top'], r['datasets'])
    print()


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument('path', type=Path, help='metadata.jsonl from gov-data')
    ap.add_argument('--limit', type=int, default=0, help='stop after N records (0 = all)')
    ap.add_argument(
        '--sample-every', type=int, default=1, help='only process every Nth record (default 1)'
    )
    ap.add_argument(
        '--top', type=int, default=30, help='how many rows in each top-N table (default 30)'
    )
    ap.add_argument(
        '--json', type=Path, default=None, help='also write the full summary as JSON to this path'
    )
    ap.add_argument(
        '--progress-every',
        type=int,
        default=25000,
        help='print a progress line every N records read',
    )
    args = ap.parse_args(argv)

    if not args.path.exists():
        print(f'error: {args.path} does not exist', file=sys.stderr)
        return 1

    stats = Stats()
    read = bad = 0
    with args.path.open('r', encoding='utf-8') as f:
        for read, line in enumerate(f, 1):
            if args.sample_every > 1 and (read - 1) % args.sample_every:
                continue
            line = line.strip()
            if not line:
                continue
            try:
                d = json.loads(line)
            except json.JSONDecodeError:
                bad += 1
                continue
            stats.add_record(d)
            if args.progress_every and read % args.progress_every == 0:
                print(
                    f'  ...read {read:,} records ({stats.resources:,} resources)', file=sys.stderr
                )
            if args.limit and read >= args.limit:
                break

    if bad:
        print(f'note: skipped {bad:,} unparseable lines', file=sys.stderr)

    print_report(stats, args.top)

    if args.json:
        args.json.write_text(json.dumps(stats.to_dict(args.top), indent=2))
        print(f'wrote JSON summary to {args.json}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main(sys.argv[1:]))
