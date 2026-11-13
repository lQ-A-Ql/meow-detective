# Windows parser regression samples

The files retain their upstream bytes. `expected.json` records SHA-256 hashes and independent expected fields. Tests read these public samples locally and never depend on workstation evidence.

| Files | Origin and fixed revision | License and oracle |
|---|---|---|
| `notepad.lnk`, `notepad-xp.lnk` | [Sootmark/shell](https://github.com/Sootmark/shell/tree/dc1633789085db979ee64b3cdafcf4ee8d138889/tests/fixtures/lnk), `Win10/Notepad.lnk` and `WinXP/Notepad.lnk` | Eric Zimmerman's public test set, MIT, `LICENSE-zimmerman.txt`; expected fields from the revision's `tests/fixtures/oracle/LECmd.csv` |
| `f01b4d95cf55d32a.automaticDestinations-ms` | Same revision, `tests/fixtures/jumplist/Win10/` | MIT; nine entries in the revision's `JLECmd.csv` |
| `368d807282ccde9d.customDestinations-ms` | Same revision, `tests/fixtures/jumplist/plaso/` | Apache-2.0, `LICENSE-plaso.txt`; three links in five categories, independently checked by JLECmd and the upstream category test |
| `recycle_v1.bin`, `recycle_v2.bin` | [SecurityRonin/trash-forensic](https://github.com/SecurityRonin/trash-forensic/tree/055b39ef92dffc20a57f6456303f6e33fa08a03b/tests/data), `$IZZ99YY.txt` and `$IAB12CD.docx` | Apache-2.0, `LICENSE-trash.txt`; synthetic standard records independently checked with rifiuti2 0.8.2; no `$R` content is included |
| `thumbcache_v21.db`, `thumbcache_v32.db` | [Sootmark/imagecache](https://github.com/Sootmark/imagecache/tree/a2727648732c28da2808ddd579d6dc687f400000/tests/fixtures/synthetic/thumbcache), `windows7/thumbcache_256.db` and `windows10/thumbcache_256.db` | MIT, `LICENSE-sootmark.txt`, `NOTICE-imagecache.txt`; independent dissect.thumbcache and CRC oracles |
| `SRUDB.dat.gz` | [Sootmark/ese](https://github.com/Sootmark/ese/tree/3570ed9280f73d0d4796ed8979a7a4e30ea022d9/tests/fixtures/plaso), originally plaso `a70dde825618050f6ce6c646f12fed386a9b4d73/test_data/SRUDB.dat` | Apache-2.0, `LICENSE-plaso.txt`, `NOTICE-ese.txt`; upstream comparison against libesedb and plaso |

The thumbnail samples deliberately include one bad data checksum and one bad header checksum. Their eight records and 2,597 payload bytes must remain visible together with two diagnostics; neither sample is a wholly verified cache. The valid Win10 LNK has unset target times and therefore yields no timeline events. The WinXP LNK retains all three target times.

The WinXP LNK access time is independently verified with `LnkParse3 1.6.0` through its Python API: little-endian bytes `76 ad 8e 7e f4 4d d1 01`, FILETIME `130971576661093750`, UTC `2016-01-13T11:21:06.109375+00:00`. The LECmd CSV records whole seconds; fractional precision comes from the raw FILETIME and the independent API result. The oracle wheel SHA-256 is `7b3b1289e99992a4d3901d80ac9ab87fa8de854b2bffe5c4011453b093038b50`.

The SRU regression checks file/header metadata. It does not claim SRUM table extraction or transaction-log replay. Gzip compression is transport storage only; tests use the shared `flate2` decoder.

The decompressed SRU fixture was independently checked with Windows `esentutl /mh`: format `0x620,20`, page size 4096, state `Clean Shutdown`, expected and actual header checksum `0xee5eb5ce`, log required `0-0`. Its decompressed SHA-256 is `6536ae6bb5b91f6f8f37a4af26f6cfaecc8a1f745370bfba83af7ebae6694e3e`.
