"""Generate method-14 fixtures independently with Python zipfile and liblzma.

Compile no_eos.c with liblzma, then pass the executable path as argv[1].
"""
import io
from pathlib import Path
import struct
import subprocess
import sys
import zipfile
import zlib

root = Path(__file__).parent
payload = bytes(range(256)) * 160
with zipfile.ZipFile(root / 'python-eos.zip', 'w', compression=zipfile.ZIP_LZMA) as archive:
    for name, content in [('payload.bin', payload), ('empty.bin', b''), ('expansion.bin', b'a' * (9 * 1024 * 1024))]:
        info = zipfile.ZipInfo(name, (2020, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_LZMA
        archive.writestr(info, content)

with zipfile.ZipFile(root / 'python-eos.zip') as archive:
    assert archive.read('payload.bin') == payload
    assert archive.read('empty.bin') == b''
    assert archive.read('expansion.bin') == b'a' * (9 * 1024 * 1024)

for empty in [False, True]:
    content = b'' if empty else b'ZIP LZMA without an end marker.\n'
    encoded = subprocess.check_output([sys.argv[1]] + (['empty'] if empty else []))
    body = bytes([4, 3, 5, 0]) + encoded
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, 'w') as archive:
        archive.writestr(zipfile.ZipInfo('payload.bin', (2020, 1, 1, 0, 0, 0)), body)
    data = bytearray(buffer.getvalue())
    central = data.index(b'PK\x01\x02')
    for offset in [4, central + 6]:
        struct.pack_into('<H', data, offset, 63)
    for offset in [8, central + 10]:
        struct.pack_into('<H', data, offset, 14)
    for offset in [14, central + 16]:
        struct.pack_into('<I', data, offset, zlib.crc32(content))
    for offset in [22, central + 24]:
        struct.pack_into('<I', data, offset, len(content))
    (root / ('liblzma-no-eos-empty.zip' if empty else 'liblzma-no-eos.zip')).write_bytes(data)
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        assert archive.read('payload.bin') == content
print('Generated and independently decoded all fixtures.')
