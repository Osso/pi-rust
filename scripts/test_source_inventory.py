"""Behavioral fixtures for re-pin selection, statuses and validation."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("inventory", Path(__file__).with_name("source-inventory.py"))
inventory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inventory)


class InventoryTests(unittest.TestCase):
    def test_selection_and_line_counts(self):
        files = {
            "packages/a/src/a.ts": b"a\nb", "packages/a/src/data.json": b"{}\n",
            "packages/a/examples/overlay-test.ts": b"x\n",
            "packages/a/test/a.ts": b"ignored", "packages/a/src/a.test.ts": b"ignored",
            "packages/a/package.json": b"{}", "packages/a/readme.md": b"doc",
        }
        expected, excluded = inventory.select_sources(files)
        self.assertEqual(expected, {"a/src/a.ts": 2, "a/src/data.json": 1, "a/examples/overlay-test.ts": 1})
        self.assertEqual(excluded["test"], 2)
        with self.assertRaisesRegex(ValueError, "Unclassified"):
            inventory.select_sources({"packages/a/foo.xyz": b"x"})

    def test_repin_preserves_only_unchanged_and_records_removed(self):
        old = {name: [name, "1", "0", "done", "`crates/a.rs`", "proof"]
               for name in ["a/same.ts", "a/change.ts", "a/old.ts", "a/gone.ts"]}
        changes = [["M", ["packages/a/change.ts"]], ["R", ["packages/a/old.ts", "packages/a/new.ts"]],
                   ["D", ["packages/a/gone.ts"]], ["A", ["packages/a/add.ts"]]]
        expected = {"a/same.ts": 1, "a/change.ts": 2, "a/new.ts": 2, "a/add.ts": 2}
        rows, removed = inventory.repin_rows(old, expected, changes)
        rows = {row[0]: row for row in rows}
        self.assertEqual(rows["a/same.ts"][3], "done")
        self.assertIn("re-run vs 4f9e6f8a4 pending", rows["a/same.ts"][5])
        for name in ["a/change.ts", "a/new.ts"]:
            self.assertEqual(rows[name][3], "todo")
            self.assertIn("old status: done", rows[name][5])
            self.assertEqual(rows[name][4], "`crates/a.rs`")
        self.assertEqual(rows["a/add.ts"][3:], ["todo", "—", "new on master"])
        self.assertEqual([row[0] for row in removed], ["a/gone.ts"])
        text = inventory.render(list(rows.values()), removed)
        inventory.validate(text, expected)
        for mutated in [text.replace("| 2 |", "| 9 |", 1),
                        text.replace("| done |", "| invalid |", 1),
                        text.replace("4 unique files", "5 unique files"),
                        text.replace("`done` 1", "`done` 8"),
                        text.replace("## Removed by re-pin", "| `a/same.ts` | 1 | 0 | done | — | duplicate |\n\n## Removed by re-pin")]:
            with self.assertRaises((AssertionError, ValueError)):
                inventory.validate(mutated, expected)

    def test_legacy_mapping_column_is_normalized_without_losing_proof(self):
        old = {"a/change.ts": ["a/change.ts", "1", "`crates/a.rs`", "done", "real compiler proof", "notes"]}
        rows, removed = inventory.repin_rows(old, {"a/change.ts": 2}, [["M", ["packages/a/change.ts"]]])
        self.assertEqual(rows[0][2], "—")
        self.assertEqual(rows[0][4], "`crates/a.rs`")
        self.assertIn("real compiler proof", rows[0][5])
        self.assertIn("old Rust: `crates/a.rs`", rows[0][5])


if __name__ == "__main__":
    unittest.main()
