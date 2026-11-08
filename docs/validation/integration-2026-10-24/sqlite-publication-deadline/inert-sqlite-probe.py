"""Owned SQLite C-API observations, never native Store/service acceptance."""
import ctypes
import json
import os
from pathlib import Path
import shutil
import tempfile
import time


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def main():
    require(os.name == 'nt', 'This fixed probe selects the installed Windows Python SQLite DLL')
    require(shutil.disk_usage(Path(__file__).resolve().parent).free >= 500_000_000,
            'Owned SQLite observation disk reserve unavailable')
    import _sqlite3
    library = Path(_sqlite3.__file__).resolve().parent / 'sqlite3.dll'
    require(library.is_file() and not library.is_symlink() and not library.is_junction(),
            'Installed Python SQLite DLL unavailable')
    api = ctypes.CDLL(str(library))
    busy_type = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_int)
    row_type = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_int,
                               ctypes.POINTER(ctypes.c_char_p), ctypes.POINTER(ctypes.c_char_p))
    api.sqlite3_open_v2.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p), ctypes.c_int, ctypes.c_char_p]
    api.sqlite3_open_v2.restype = ctypes.c_int
    api.sqlite3_exec.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_void_p,
                                ctypes.POINTER(ctypes.c_void_p)]
    api.sqlite3_exec.restype = ctypes.c_int
    api.sqlite3_busy_timeout.argtypes = [ctypes.c_void_p, ctypes.c_int]
    api.sqlite3_busy_timeout.restype = ctypes.c_int
    api.sqlite3_busy_handler.argtypes = [ctypes.c_void_p, busy_type, ctypes.c_void_p]
    api.sqlite3_busy_handler.restype = ctypes.c_int
    api.sqlite3_close.argtypes = [ctypes.c_void_p]
    api.sqlite3_close.restype = ctypes.c_int
    api.sqlite3_free.argtypes = [ctypes.c_void_p]
    api.sqlite3_libversion.restype = ctypes.c_char_p
    api.sqlite3_sourceid.restype = ctypes.c_char_p

    def execute(db, statement, callback=None):
        error = ctypes.c_void_p()
        try:
            return api.sqlite3_exec(db, statement, ctypes.cast(callback, ctypes.c_void_p) if callback else None,
                                    None, ctypes.byref(error))
        finally:
            if error:
                api.sqlite3_free(error)

    def busy_value(db):
        values = []

        @row_type
        def capture(_data, columns, row, _names):
            if columns != 1 or len(values) >= 1:
                return 1
            values.append(row[0])
            return 0

        require(execute(db, b'PRAGMA busy_timeout', capture) == 0 and len(values) == 1,
                'Owned SQLite policy observation unavailable')
        return int(values[0])

    rows = []
    with tempfile.TemporaryDirectory(prefix='yoke-owned-sqlite-deadline-') as directory:
        root = Path(directory).resolve()
        require(shutil.disk_usage(root).free >= 500_000_000, 'Owned fixture disk reserve unavailable')
        handles = []
        try:
            for _ in range(2):
                db = ctypes.c_void_p()
                code = api.sqlite3_open_v2(str(root / 'owned.sqlite').encode('utf-8'), ctypes.byref(db), 6, None)
                if db:
                    handles.append(db)
                require(code == 0 and db, 'Owned SQLite connection unavailable')
            owner, contender = handles
            require(execute(owner, b'PRAGMA journal_mode=WAL; CREATE TABLE owned(value INTEGER)') == 0,
                    'Owned SQLite schema unavailable')
            require(execute(owner, b'BEGIN IMMEDIATE') == 0, 'Owned writer lock unavailable')
            for reinstall in (False, True):
                state = {'calls': 0, 'end': float('inf')}

                @busy_type
                def deadline_wait(_data, _count):
                    state['calls'] += 1
                    remaining = state['end'] - time.monotonic()
                    if remaining <= 0:
                        return 0
                    time.sleep(min(.002, remaining))
                    return int(time.monotonic() < state['end'])

                require(api.sqlite3_busy_handler(contender, deadline_wait, None) == 0,
                        'Owned callback registration unavailable')
                require(api.sqlite3_busy_timeout(contender, 777) == 0, 'Owned caller policy unavailable')
                require(busy_value(contender) == 777, 'Caller policy differs')
                started = time.monotonic()
                state['end'] = started + .060
                if reinstall:
                    require(api.sqlite3_busy_handler(contender, deadline_wait, None) == 0,
                            'Owned callback reinstallation unavailable')
                inside = busy_value(contender)
                code = execute(contender, b'BEGIN IMMEDIATE')
                elapsed = time.monotonic() - started
                require(code == 5 and elapsed < 3, 'Owned contention observation differs')
                require((state['calls'] > 0) == reinstall, 'Observed callback selection differs')
                require(api.sqlite3_busy_timeout(contender, 777) == 0, 'Caller policy restoration failed')
                restored = busy_value(contender)
                require(restored == 777, 'Caller policy was not restored')
                rows.append({'reinstalledAfterBuiltinTimeout': reinstall, 'callerBusyMs': 777,
                    'requestedDeadlineMs': 60, 'insideBuiltinBusyMs': inside, 'callbackCalls': state['calls'],
                    'sqliteResult': code, 'elapsedSeconds': elapsed, 'restoredBusyMs': restored,
                    'nativeAcceptance': False})
            require(execute(owner, b'ROLLBACK') == 0, 'Owned writer rollback failed')
        finally:
            failures = [api.sqlite3_close(db) for db in reversed(handles)]
            require(all(code == 0 for code in failures), 'Owned SQLite connection cleanup failed')
    print(json.dumps({'diagnosticOnly': True, 'nativeAcceptance': False,
        'nativeYokeImageReadOrExecuted': False, 'library': str(library),
        'sqliteVersion': api.sqlite3_libversion().decode('ascii'),
        'sqliteSourceId': api.sqlite3_sourceid().decode('ascii'),
        'scope': 'Actual installed Python SQLite C API, not bundled native Rust SQLite',
        'allOwnedConnectionsClosed': True, 'trials': rows}, indent=2))


if __name__ == '__main__':
    main()
