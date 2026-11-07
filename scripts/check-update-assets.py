#!/usr/bin/env python3
"""Fail a release before publishing if the app's updater cannot consume its assets."""

import plistlib
import sys
import tarfile
import tomllib
import zipfile
from pathlib import Path


def check_assets(directory: Path, version: str) -> None:
    for arch in ('universal', 'aarch64', 'x86_64'):
        stem = f'Paper-Shell-macOS-{arch}'
        if not (directory / f'{stem}.dmg').is_file():
            raise ValueError(f'Missing {stem}.dmg')
        with zipfile.ZipFile(directory / f'{stem}.zip') as archive:
            if archive.testzip() is not None:
                raise ValueError(f'Corrupt {stem}.zip')
            binary = archive.getinfo('Paper Shell.app/Contents/MacOS/paper-shell')
            if binary.file_size == 0:
                raise ValueError(f'Empty executable in {stem}.zip')
            plist = plistlib.loads(archive.read('Paper Shell.app/Contents/Info.plist'))
            if (plist.get('CFBundleIdentifier'), plist.get('CFBundleShortVersionString')) != ('paper.shell.app', version):
                raise ValueError(f'Bundle identity/version mismatch in {stem}.zip')
    with zipfile.ZipFile(directory / 'Paper-Shell-Windows-x86_64.zip') as archive:
        if archive.testzip() is not None or archive.getinfo('paper-shell.exe').file_size == 0:
            raise ValueError('Invalid Windows update archive')
    with tarfile.open(directory / 'Paper-Shell-Linux-x86_64.tar.gz') as archive:
        entries = archive.getmembers()
        if len(entries) != 1 or entries[0].name != 'paper-shell' or not entries[0].isfile() or entries[0].size == 0:
            raise ValueError('Invalid Linux update archive')


if __name__ == '__main__':
    version = tomllib.loads(Path('Cargo.toml').read_text())['package']['version']
    check_assets(Path(sys.argv[1]), version)
    print(f'Updater assets match version {version}')
