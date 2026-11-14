#!/usr/bin/env python3
"""Fails when Krozties could do more than read the game's pixels.

The combat reading only reads the pixels of game windows. This check fails the
build when the code, the dependencies or the macOS binary reference:

* input synthesis (posting key or mouse events),
* access to another process's memory,
* packet capture,
* the Accessibility or Input Monitoring permissions,
* a network library under the reading crate, whose frames never leave the
  machine.

Usage:
    read_only_check.py              sources and Cargo.lock, from the repository
    read_only_check.py --binary P   the symbols the macOS binary P imports
    read_only_check.py --test       the check's own tests
"""

from __future__ import annotations

import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Identifiers no Rust source of Krozties may name, by what they would allow.
SOURCE_RULES: dict[str, list[str]] = {
    'input synthesis': [
        'CGEventPost', 'CGEventPostToPid', 'CGEventPostToPSN', 'CGEventCreateKeyboardEvent',
        'CGEventCreateMouseEvent', 'CGEventCreateScrollWheelEvent', 'CGEventKeyboardSetUnicodeString',
        'SendInput', 'keybd_event', 'mouse_event', 'PostMessageW', 'PostMessageA', 'SendMessageW',
        'SendMessageA', 'XTestFakeKeyEvent', 'XTestFakeButtonEvent', 'uinput',
    ],
    'process memory': [
        'ReadProcessMemory', 'WriteProcessMemory', 'VirtualAllocEx', 'CreateRemoteThread', 'task_for_pid',
        'mach_vm_read', 'mach_vm_read_overwrite', 'mach_vm_write', 'vm_read_overwrite', 'process_vm_readv',
        'process_vm_writev', 'ptrace',
    ],
    'packet capture': ['pcap_open_live', 'pcap_create', 'pcap_open', 'WinDivertOpen', 'AF_PACKET', 'SOCK_RAW'],
    'accessibility or input monitoring': [
        'AXUIElementCreateApplication', 'AXUIElementCopyAttributeValue', 'AXIsProcessTrusted',
        'AXIsProcessTrustedWithOptions', 'IOHIDManagerCreate', 'CGEventTapCreate',
        'addGlobalMonitorForEventsMatchingMask', 'kTCCServiceAccessibility', 'kTCCServiceListenEvent',
    ],
}

# Crates no part of Krozties may depend on.
FORBIDDEN_CRATES = {
    'enigo': 'input synthesis', 'rdev': 'input monitoring and synthesis', 'inputbot': 'input synthesis',
    'autopilot': 'input synthesis', 'mouce': 'input synthesis', 'device_query': 'input monitoring',
    'pcap': 'packet capture', 'pnet': 'packet capture', 'pnet_datalink': 'packet capture',
    'windivert': 'packet capture', 'windivert-sys': 'packet capture',
    'read-process-memory': 'process memory', 'process-memory': 'process memory',
}

# Network crates the reading crate may not reach, even through a dependency.
NETWORK_CRATES = {
    'reqwest', 'hyper', 'ureq', 'isahc', 'curl', 'surf', 'h2', 'tungstenite', 'tokio-tungstenite', 'quinn',
    'socket2', 'attohttpc',
}
READING_CRATE = 'dofus-vision'

# Symbols the macOS binary may not import, by what they would allow.
BINARY_RULES: dict[str, list[str]] = {
    'input synthesis': [
        r'_CGEventPost\w*', r'_CGEventCreateKeyboardEvent', r'_CGEventCreateMouseEvent',
        r'_CGEventCreateScrollWheelEvent\w*', r'_CGEventKeyboardSetUnicodeString',
    ],
    'process memory': [r'_task_for_pid', r'_mach_vm_read\w*', r'_mach_vm_write', r'_vm_read\w*', r'_ptrace'],
    'packet capture': [r'_pcap_\w+'],
    'accessibility or input monitoring': [r'_AXUIElement\w+', r'_AXIsProcessTrusted\w*', r'_IOHIDManager\w+'],
}

# Imported symbols allowed, each with the reason it is harmless.
ALLOWED_SYMBOLS = {
    '_CGEventTapCreate': (
        "global-hotkey, behind the companion's shortcuts, taps events for media keys only; the companion refuses "
        'media keys (test une_touche_multimedia_se_refuse), so the tap and its permission are never reached'
    ),
    '_CGEventTapEnable': 'same as _CGEventTapCreate',
}


def scan_sources(root: Path) -> list[str]:
    """The forbidden identifiers named in Rust sources under `crates/`."""
    findings = []
    patterns = {
        category: re.compile(r'\b(' + '|'.join(re.escape(n) for n in names) + r')\b')
        for category, names in SOURCE_RULES.items()
    }
    for path in sorted((root / 'crates').rglob('*.rs')):
        if 'target' in path.parts:
            continue
        for number, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
            for category, pattern in patterns.items():
                match = pattern.search(line)
                if match:
                    findings.append(f'{path.relative_to(root)}:{number}: {match.group(1)} ({category})')
    return findings


def lock_packages(lock: Path) -> dict[str, list[str]]:
    """Each package of a Cargo.lock and the names of its dependencies."""
    data = tomllib.loads(lock.read_text(encoding='utf-8'))
    return {p['name']: [d.split(' ')[0] for d in p.get('dependencies', [])] for p in data.get('package', [])}


def scan_lock(lock: Path) -> list[str]:
    """Forbidden crates anywhere, and network crates under the reading crate."""
    packages = lock_packages(lock)
    findings = [f'Cargo.lock: crate {n} ({FORBIDDEN_CRATES[n]})' for n in sorted(packages) if n in FORBIDDEN_CRATES]
    if READING_CRATE in packages:
        seen, todo = set(), [READING_CRATE]
        while todo:
            name = todo.pop()
            if name in seen:
                continue
            seen.add(name)
            todo.extend(packages.get(name, []))
        findings += [f'Cargo.lock: {READING_CRATE} reaches the network crate {n}' for n in sorted(seen & NETWORK_CRATES)]
    return findings


def check_symbols(symbols: list[str]) -> tuple[list[str], list[str]]:
    """The forbidden imported symbols, and the allowed ones met with why."""
    findings, allowed = [], []
    rules = [(category, re.compile(pattern)) for category, patterns in BINARY_RULES.items() for pattern in patterns]
    for symbol in sorted(set(symbols)):
        if symbol in ALLOWED_SYMBOLS:
            allowed.append(f'{symbol}: {ALLOWED_SYMBOLS[symbol]}')
            continue
        for category, pattern in rules:
            if pattern.fullmatch(symbol):
                findings.append(f'binary imports {symbol} ({category})')
                break
    return findings, allowed


def imported_symbols(binary: Path) -> list[str]:
    output = subprocess.run(['nm', '-u', str(binary)], capture_output=True, text=True, check=True).stdout
    return [line.strip() for line in output.splitlines() if line.strip().startswith('_')]


def report(findings: list[str]) -> int:
    for f in findings:
        print(f'FORBIDDEN {f}')
    print('read-only check: ' + ('failed' if findings else 'passed'))
    return 1 if findings else 0


def self_test() -> int:
    failures = []

    def expect(name, condition):
        print(('ok     ' if condition else 'FAILED ') + name)
        if not condition:
            failures.append(name)

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / 'crates' / 'a' / 'src').mkdir(parents=True)
        source = root / 'crates' / 'a' / 'src' / 'lib.rs'
        for category, names in SOURCE_RULES.items():
            source.write_text(f'fn f() {{ unsafe {{ {names[0]}(); }} }}\n')
            expect(f'a source naming {names[0]} fails ({category})', len(scan_sources(root)) == 1)
        source.write_text('// Reads the pixels of game windows only.\nfn capture() {}\n')
        expect('a clean source passes', scan_sources(root) == [])
        source.write_text('fn f() { let x = CGEventPosterior; }\n')
        expect('a longer identifier is not a match', scan_sources(root) == [])

        lock = root / 'Cargo.lock'
        lock.write_text(
            '[[package]]\nname = "dofus-vision"\nversion = "0.1.0"\ndependencies = ["png"]\n\n'
            '[[package]]\nname = "png"\nversion = "0.17.0"\n\n'
            '[[package]]\nname = "dofus-app"\nversion = "0.1.0"\ndependencies = ["reqwest"]\n\n'
            '[[package]]\nname = "reqwest"\nversion = "0.12.0"\n'
        )
        expect('a network crate outside the reading crate passes', scan_lock(lock) == [])
        lock.write_text(lock.read_text().replace('dependencies = ["png"]', 'dependencies = ["png", "dofus-app"]'))
        expect('a network crate reached by the reading crate fails', len(scan_lock(lock)) == 1)
        lock.write_text(lock.read_text() + '\n[[package]]\nname = "enigo"\nversion = "0.2.0"\n')
        expect('a forbidden crate fails', any('enigo' in f for f in scan_lock(lock)))

    findings, allowed = check_symbols(['_CGEventPost', '_CGWindowListCopyWindowInfo', '_CGEventTapCreate'])
    expect('an imported _CGEventPost fails', findings == ['binary imports _CGEventPost (input synthesis)'])
    expect('the hotkey tap is allowed, with its reason', len(allowed) == 1 and 'media keys' in allowed[0])
    findings, _ = check_symbols(['_mach_vm_read_overwrite', '_pcap_open_live', '_AXUIElementCreateApplication'])
    expect('memory, capture and accessibility symbols fail', len(findings) == 3)
    findings, _ = check_symbols(['_SCStreamFrameInfoStatus', '_CGEventSourceCreate', '_objc_msgSend'])
    expect('capture and unrelated symbols pass', findings == [])

    print('self-test: ' + ('failed' if failures else 'passed'))
    return 1 if failures else 0


def main(arguments: list[str]) -> int:
    if arguments[:1] == ['--test']:
        return self_test()
    if arguments[:1] == ['--binary'] and len(arguments) == 2:
        findings, allowed = check_symbols(imported_symbols(Path(arguments[1])))
        for a in allowed:
            print(f'allowed {a}')
        return report(findings)
    if arguments:
        print(__doc__)
        return 2
    return report(scan_sources(ROOT) + scan_lock(ROOT / 'Cargo.lock'))


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
