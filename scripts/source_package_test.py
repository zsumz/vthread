"""Negative source-distribution tests: reject tampering and unsafe archive extraction."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import warnings
import zipfile

from source_package import MAX_UNCOMPRESSED_BYTES, MANIFEST, digest, verify_extract


class SourcePackageTest(unittest.TestCase):
    def archive(self, folder, files, manifest_files=None):
        archive = Path(folder) / "source.zip"
        inventory = manifest_files if manifest_files is not None else {
            name: digest(content) for name, content in files.items()}
        with zipfile.ZipFile(archive, "w") as zipped:
            for name, content in files.items():
                zipped.writestr(name, content)
            zipped.writestr(MANIFEST, json.dumps({"schema": 1, "files": inventory}))
        return archive

    def test_changed_or_unlisted_source_is_rejected_before_extraction(self):
        for inventory in ({"src/lib.rs": digest(b"original")}, {}):
            with tempfile.TemporaryDirectory() as folder:
                archive = self.archive(folder, {"src/lib.rs": b"changed"}, inventory)
                target = Path(folder) / "extracted"
                with self.assertRaises(ValueError):
                    verify_extract(archive, target, digest(archive.read_bytes()))
                self.assertFalse(target.exists())

    def test_oversized_declared_content_is_rejected_before_any_extraction(self):
        with tempfile.TemporaryDirectory() as folder:
            archive = self.archive(folder, {"source.rs": b"small compressed input"})
            target = Path(folder) / "extracted"
            inflated = zipfile.ZipInfo("source.rs")
            inflated.file_size = MAX_UNCOMPRESSED_BYTES + 1
            with patch("source_package.zipfile.ZipFile.infolist", return_value=[inflated]):
                with self.assertRaisesRegex(ValueError, "exceeds size limit"):
                    verify_extract(archive, target, digest(archive.read_bytes()))
            self.assertFalse(target.exists())

    def test_parent_escape_and_absolute_paths_are_rejected(self):
        for path in ("../escaped.rs", "/absolute.rs", "src/../../escaped.rs", "src\\escape.rs"):
            with tempfile.TemporaryDirectory() as folder:
                archive = self.archive(folder, {path: b"bad"})
                target = Path(folder) / "extracted"
                with self.assertRaises(ValueError):
                    verify_extract(archive, target, digest(archive.read_bytes()))
                self.assertFalse(target.exists())

    def test_external_checksum_is_required_and_valid_inventory_extracts(self):
        with tempfile.TemporaryDirectory() as folder:
            archive = self.archive(folder, {"src/lib.rs": b"verified"})
            target = Path(folder) / "extracted"
            with self.assertRaises(ValueError):
                verify_extract(archive, target, "0" * 64)
            verify_extract(archive, target, digest(archive.read_bytes()))
            self.assertEqual((target / "src/lib.rs").read_bytes(), b"verified")

    def test_symlinks_and_duplicate_entries_are_rejected(self):
        for symlink in (True, False):
            with tempfile.TemporaryDirectory() as folder:
                archive = self.archive(folder, {"source.rs": b"original"})
                with zipfile.ZipFile(archive, "a") as zipped:
                    entry = zipfile.ZipInfo("link" if symlink else "source.rs")
                    if symlink:
                        entry.external_attr = 0o120777 << 16
                    with warnings.catch_warnings():
                        warnings.simplefilter("ignore", UserWarning)
                        zipped.writestr(entry, b"source.rs")
                target = Path(folder) / "extracted"
                with self.assertRaises(ValueError):
                    verify_extract(archive, target, digest(archive.read_bytes()))
                self.assertFalse(target.exists())


if __name__ == "__main__":
    unittest.main()
