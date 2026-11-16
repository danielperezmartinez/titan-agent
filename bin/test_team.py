"""Tests of the team prototype (bin/team) as adapted to this repository.

They never call codex: they cover the configuration of this repo, the path
rules of each role and the full `new` + `ask --dry-run` path on a throwaway
git repository. Run: python3 -m unittest discover -s bin -p 'test_*.py'
"""
import contextlib
import importlib.machinery
import importlib.util
import io
import subprocess
import tempfile
import unittest
from pathlib import Path

BIN = Path(__file__).resolve().parent
ROOT = BIN.parent


def load_team():
    loader = importlib.machinery.SourceFileLoader("team", str(BIN / "team"))
    spec = importlib.util.spec_from_loader("team", loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


team = load_team()


def run(cmd, cwd):
    subprocess.run(cmd, cwd=cwd, check=True, capture_output=True, text=True)


class RepoConfigTest(unittest.TestCase):
    def setUp(self):
        self.cfg = team.load_config(ROOT / ".agents/team.toml")

    def test_project_targets_this_repo(self):
        project = self.cfg["project"]
        self.assertEqual(project["repo"], ".")
        self.assertEqual(project["base"], "main")
        self.assertEqual(project["test_cmd"], "cargo test --workspace")
        self.assertGreater(project["test_timeout_s"], 300)

    def test_flow_and_roles(self):
        flow = self.cfg["flow"]
        self.assertEqual(flow["steps"], ["disenador_tests", "revisor"])
        self.assertEqual(flow["fix_role"], "director")
        self.assertEqual(flow["max_review_rounds"], 1)
        for step in flow["steps"]:
            role = team.require_role(self.cfg, step)
            self.assertEqual(role["cli"], "codex")
            self.assertTrue((ROOT / role["prompt_file"]).is_file())
        self.assertEqual(self.cfg["roles"]["mecanico"]["model"], "gpt-6-luna")
        self.assertEqual(self.cfg["roles"]["disenador_tests"]["model"], "gpt-6.1-sol")
        self.assertEqual(self.cfg["roles"]["revisor"]["model"], "gpt-6.1-sol")
        self.assertEqual(self.cfg["roles"]["revisor"]["sandbox"], "read-only")
        self.assertTrue((ROOT / self.cfg["roles"]["revisor"]["output_schema"]).is_file())

    def test_roles_are_isolated_from_the_repo_instructions(self):
        overrides = team.codex_overrides(self.cfg, self.cfg["roles"]["mecanico"])
        self.assertIn("project_doc_max_bytes=0", overrides)
        self.assertIn("skills.include_instructions=false", overrides)
        hidden = self.cfg["project"]["hidden_paths"]
        for path in ("/AGENTS.md", "/CLAUDE.md", "/GEMINI.md", "/docs/", "/.claude/"):
            self.assertIn(path, hidden)

    def test_director_cannot_be_launched(self):
        with self.assertRaises(team.TeamError):
            team.require_role(self.cfg, "director")


class LaunchRulesTest(unittest.TestCase):
    def test_claude_roles_are_never_launched(self):
        cfg = {"roles": {"r": {"cli": "claude", "model": "opus"}}}
        with self.assertRaises(team.TeamError):
            team.require_role(cfg, "r")
        self.assertEqual(team.require_role(cfg, "r", launch=False)["cli"], "claude")

    def test_claude_adapter_is_gone(self):
        self.assertFalse(hasattr(team, "claude_cmd"))
        self.assertFalse(hasattr(team, "parse_claude"))


class PathRulesTest(unittest.TestCase):
    def test_trailing_slash_covers_the_whole_directory(self):
        self.assertTrue(team.path_matches("crates/core/tests/a.rs", "crates/*/tests/"))
        self.assertTrue(team.path_matches("crates/core/tests/sub/b.rs", "crates/*/tests/"))
        self.assertFalse(team.path_matches("crates/core/src/lib.rs", "crates/*/tests/"))
        self.assertFalse(team.path_matches("tests/a.rs", "crates/*/tests/"))

    def test_plain_prefix_still_works(self):
        self.assertTrue(team.path_matches("tests/a.py", "tests/"))
        self.assertFalse(team.path_matches("src/tests.py", "tests/"))

    def test_warnings(self):
        designer = {"allowed_paths": ["crates/*/tests/"]}
        implementer = {"forbidden_paths": ["crates/*/tests/"]}
        files = ["crates/core/tests/a.rs", "crates/core/src/lib.rs"]
        self.assertEqual(team.path_warnings(designer, files),
                         ["Fichero fuera de allowed_paths: crates/core/src/lib.rs"])
        self.assertEqual(team.path_warnings(implementer, files),
                         ["Fichero dentro de forbidden_paths: crates/core/tests/a.rs"])
        self.assertEqual(team.path_warnings({}, files), [])


class CodexOverridesTest(unittest.TestCase):
    def test_global_then_role_overrides(self):
        cfg = {"codex": {"config": ["a=1"]}}
        self.assertEqual(team.codex_overrides(cfg, {"config": ["b=2"]}), ["-c", "a=1", "-c", "b=2"])
        self.assertEqual(team.codex_overrides({}, {}), [])

    def test_overrides_reach_new_and_resumed_threads(self):
        role = {"model": "m", "sandbox": "read-only"}
        out = Path("out")
        for thread in (None, "thread-1"):
            cmd = team.codex_cmd(role, thread, out, Path("wt"), ["-c", "x=1"])
            self.assertIn("x=1", cmd)
            self.assertEqual(cmd[cmd.index("x=1") - 1], "-c")


class DryRunTest(unittest.TestCase):
    """new + ask --dry-run on a throwaway repo laid out like this one."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name) / "repo"
        self.base.mkdir()
        (self.base / ".agents").mkdir()
        for rel in (".agents/team.toml", ".agents/prompts/disenador_tests.md",
                    ".agents/prompts/mecanico.md", ".agents/prompts/revisor.md",
                    ".agents/schemas/review.json"):
            dest = self.base / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes((ROOT / rel).read_bytes())
        (self.base / ".gitignore").write_text(".agents/state/\n.agents/worktrees/\n")
        for rel in ("AGENTS.md", "CLAUDE.md", "docs/README.md", "src/main.rs"):
            dest = self.base / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(f"{rel}\n")
        run(["git", "init", "-q", "-b", "main"], self.base)
        run(["git", "add", "-A"], self.base)
        run(["git", "-c", "user.name=test", "-c", "user.email=test@example.invalid",
             "commit", "-q", "-m", "init"], self.base)
        self.brief = Path(self.tmp.name) / "brief.md"
        self.brief.write_text("# Encargo de prueba\n\nNada que hacer.\n")
        self.config = str(self.base / ".agents/team.toml")

    def tearDown(self):
        self.tmp.cleanup()

    def team(self, *argv):
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
            code = team.main(["--config", self.config, *argv])
        return code, out.getvalue()

    def test_new_and_dry_run_ask(self):
        code, _ = self.team("new", "prueba", "--brief", str(self.brief))
        self.assertEqual(code, 0)
        worktree = self.base / ".agents/worktrees/prueba"
        self.assertTrue(worktree.is_dir())
        state = team.load_state(team.load_config(Path(self.config)), "prueba")
        self.assertEqual(state["branch"], "encargo/prueba")

        code, out = self.team("ask", "disenador_tests", "prueba", "--dry-run")
        self.assertEqual(code, 0)
        self.assertIn("codex exec --json", out)
        self.assertIn("-m gpt-6.1-sol", out)
        self.assertIn(str(worktree), out)
        self.assertIn("Nada que hacer.", out)
        self.assertIn("-c project_doc_max_bytes=0", out)

        code, out = self.team("ask", "revisor", "prueba", "--dry-run", "-m", "Revisa.")
        self.assertEqual(code, 0)
        self.assertIn("-m gpt-6.1-sol", out)
        self.assertIn("-s read-only", out)
        self.assertIn("--output-schema", out)

    def test_worktree_hides_the_repo_instructions(self):
        self.team("new", "prueba", "--brief", str(self.brief))
        worktree = self.base / ".agents/worktrees/prueba"
        for rel in ("AGENTS.md", "CLAUDE.md", "docs"):
            self.assertFalse((worktree / rel).exists(), rel)
        self.assertTrue((worktree / "src/main.rs").is_file())
        self.assertTrue((worktree / ".agents/team.toml").is_file())
        # A role's commit (git add -A, as autocommit does) keeps the hidden files.
        (worktree / "src/lib.rs").write_text("lib\n")
        run(["git", "add", "-A"], worktree)
        run(["git", "-c", "user.name=test", "-c", "user.email=test@example.invalid",
             "commit", "-q", "-m", "turn"], worktree)
        tracked = subprocess.run(["git", "ls-files"], cwd=worktree, check=True,
                                 capture_output=True, text=True).stdout.split()
        for rel in ("AGENTS.md", "CLAUDE.md", "docs/README.md", "src/lib.rs"):
            self.assertIn(rel, tracked)
        # The main checkout is untouched.
        self.assertTrue((self.base / "docs/README.md").is_file())

    def test_new_from_another_base(self):
        run(["git", "checkout", "-q", "-b", "fase"], self.base)
        (self.base / "phase.txt").write_text("phase work\n")
        run(["git", "add", "-A"], self.base)
        run(["git", "-c", "user.name=test", "-c", "user.email=test@example.invalid",
             "commit", "-q", "-m", "phase"], self.base)
        run(["git", "checkout", "-q", "main"], self.base)
        code, _ = self.team("new", "prueba", "--brief", str(self.brief), "--base", "fase")
        self.assertEqual(code, 0)
        state = team.load_state(team.load_config(Path(self.config)), "prueba")
        self.assertEqual(state["base"], "fase")
        self.assertTrue((self.base / ".agents/worktrees/prueba/phase.txt").is_file())

    def test_director_ask_is_rejected(self):
        self.team("new", "prueba", "--brief", str(self.brief))
        code, _ = self.team("ask", "director", "prueba", "--dry-run")
        self.assertEqual(code, 2)


if __name__ == "__main__":
    unittest.main()
