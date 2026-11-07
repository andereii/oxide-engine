
"""Static checks for the repository's OpenCode V2 workflow contract.

Uses stdlib plus the already-optional PyYAML dependency for Markdown frontmatter.
No network, model calls, Git writes, permission execution or runtime probes occur.
fnmatchcase is used only for direct static pattern checks, NOT to simulate the
OpenCode evaluator. A pass proves neither runtime compatibility nor confinement;
shell deny rules are application policy, not an OS sandbox. Live capabilities
must be verified separately against the installed OpenCode (currently 2.2.5).
"""

import argparse
import fnmatch
import json
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
AGENT_DIR = ".opencode/agents/"
COMMAND_DIR = ".opencode/commands/"
MODELS = {
    "oxide-orchestrator": "opencode-go/glm-5.3-flash",
    "oxide-orchestrator-deep": "opencode-go/glm-5.3",
    "oxide-orchestrator-argon": "opencode-go/gemini-4-argon",
    "oxide-impl-rust": "opencode-go/deepseek-v4.1-flash#max",
    "oxide-reviewer": "opencode-go/deepseek-v4.1-flash#max",
}
POLICIES = ("AGENTS.md", "docs/GOAL.md", "docs/WORKFLOW.md")
AGENT_FIELDS = {"description", "mode", "model", "steps", "temperature", "top_p",
                "permissions", "hidden", "color", "disable"}
COMMAND_FIELDS = {"description", "agent", "model", "subtask"}
ACTIONS = {"*", "read", "edit", "glob", "grep", "shell", "subagent", "webfetch",
           "websearch", "question", "skill", "todowrite", "todoread", "lsp",
           "external_directory", "doom_loop"}
NOTICE = ("STATIC LINT ONLY: JSON/YAML, references and direct fnmatch pattern checks.\n"
          "Live OpenCode runtime capabilities/compatibility have NOT been verified.\n"
          "No permission simulation or confinement proof; shell deny is not an OS sandbox.")


class LintFailure(Exception):
    pass


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key: " + key)
        result[key] = value
    return result


def yaml_loader():
    try:
        import yaml
    except ImportError as exc:
        raise LintFailure("PyYAML is unavailable; install/use the existing optional PyYAML dependency "
                          "to lint YAML frontmatter (no dependency was added)") from exc

    class UniqueLoader(yaml.SafeLoader):
        pass

    def mapping(loader, node):
        result = {}
        for key_node, value_node in node.value:
            key = loader.construct_object(key_node)
            if not isinstance(key, str) or key in result:
                raise ValueError("duplicate or non-string YAML key: {!r}".format(key))
            result[key] = loader.construct_object(value_node)
        return result

    UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, mapping)

    def load(text):
        try:
            return yaml.load(text, Loader=UniqueLoader)
        except (yaml.YAMLError, ValueError, TypeError) as exc:
            raise LintFailure("invalid YAML frontmatter: " + str(exc)) from exc
    return load


def read_frontmatter(text, load_yaml):
    lines = text.splitlines()
    if not lines or lines[0] != "---":
        raise LintFailure("missing YAML frontmatter delimiter")
    try:
        end = lines.index("---", 1)
    except ValueError as exc:
        raise LintFailure("missing closing YAML frontmatter delimiter") from exc
    metadata = load_yaml("\n".join(lines[1:end]))
    if not isinstance(metadata, dict):
        raise LintFailure("YAML frontmatter must be a mapping")
    return metadata, "\n".join(lines[end + 1:])


def valid_glob(pattern):
    """Reject malformed bracket expressions that fnmatch otherwise treats literally."""
    if not isinstance(pattern, str) or not pattern or "\x00" in pattern or "\n" in pattern:
        return False
    i = 0
    while i < len(pattern):
        if pattern[i] == "[":
            start = i + 1
            if start < len(pattern) and pattern[start] == "!":
                start += 1
            if start < len(pattern) and pattern[start] == "]":
                start += 1
            end = pattern.find("]", start)
            if end < 0 or end == i + 1:
                return False
            i = end
        i += 1
    return True


def matching(rules, action, resource):
    """Direct pattern inspection only; this is not a runtime permissions evaluator."""
    return [rule for rule in rules if rule["action"] in (action, "*")
            and fnmatch.fnmatchcase(resource, rule["resource"])]


class Linter:
    def __init__(self, root, load_yaml):
        self.root = Path(root)
        self.load_yaml = load_yaml
        self.errors = []
        self.texts = {}
        self.agents = {}
        self.commands = {}

    def error(self, path, message):
        self.errors.append("{}: {}".format(path, message))

    def text(self, path):
        try:
            text = (self.root / path).read_text(encoding="utf-8")
            if not text.strip():
                raise ValueError("file is empty")
            self.texts[path] = text
            return text
        except (OSError, ValueError) as exc:
            self.error(path, str(exc))
            return None

    def permissions(self, path, metadata):
        rules = metadata.get("permissions")
        if not isinstance(rules, list) or not rules:
            self.error(path, "OpenCode V2 requires a nonempty permissions[] array")
            return []
        result = []
        for index, rule in enumerate(rules):
            if (not isinstance(rule, dict) or set(rule) != {"action", "resource", "effect"}
                    or not isinstance(rule.get("action"), str) or rule["action"] not in ACTIONS
                    or rule.get("effect") not in ("allow", "deny", "ask")
                    or not valid_glob(rule.get("resource"))):
                self.error(path, "invalid permissions[{}] action/resource glob/effect".format(index))
            else:
                result.append(rule)
        return result

    def global_config(self):
        path = "opencode.json"
        text = self.text(path)
        if text is None:
            return
        try:
            def invalid_constant(value):
                raise ValueError("non-JSON constant: " + value)
            config = json.loads(text, object_pairs_hook=unique_object, parse_constant=invalid_constant)
            if not isinstance(config, dict):
                raise ValueError("config must be an object")
        except ValueError as exc:
            self.error(path, "invalid JSON: " + str(exc))
            return
        if "permission" in config or "plugin" in config:
            self.error(path, "use V2 permissions[] and plugins[], not singular legacy fields")
        self.permissions(path, config)
        if config.get("default_agent") != "oxide-orchestrator":
            self.error(path, "default_agent must remain oxide-orchestrator")
        if config.get("small_model") != "opencode-go/deepseek-v4.1-flash":
            self.error(path, "small_model must remain opencode-go/deepseek-v4.1-flash")
        plugins = config.get("plugins")
        fallback = [p for p in plugins if isinstance(p, dict)
                    and p.get("package") == "opencode-runtime-fallback"] if isinstance(plugins, list) else []
        if len(fallback) != 1:
            self.error(path, "plugins[] must contain exactly one opencode-runtime-fallback configuration")
            return
        options = fallback[0].get("options")
        if not isinstance(options, dict):
            self.error(path, "runtime fallback options must be an object")
            return
        models = options.get("fallback_models")
        if (not isinstance(models, list) or not models
                or any(not isinstance(m, str) or not m.strip() for m in models)):
            self.error(path, "fallback_models must be a nonempty list of model identifiers")
        attempts = options.get("max_fallback_attempts")
        if type(attempts) is not int or not 1 <= attempts <= 3:
            self.error(path, "max_fallback_attempts must be bounded to 1..3")
        if options.get("notify_on_fallback") is not True:
            self.error(path, "notify_on_fallback must be true")
        for key in ("timeout_seconds", "cooldown_seconds"):
            value = options.get(key)
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not 0 < value <= 86400:
                self.error(path, key + " must be finite, positive and bounded to at most 86400")

    def markdown(self, path, agent):
        text = self.text(path)
        if text is None:
            return None
        try:
            data, _body = read_frontmatter(text, self.load_yaml)
        except LintFailure as exc:
            self.error(path, str(exc))
            return None
        allowed = AGENT_FIELDS if agent else COMMAND_FIELDS
        unknown = set(data) - allowed
        if unknown:
            self.error(path, "unsupported V2 frontmatter fields: " + ", ".join(sorted(map(str, unknown))))
        if not isinstance(data.get("description"), str) or not data["description"].strip():
            self.error(path, "description must be a nonempty string")
        for field in ("model", "agent", "color"):
            if field in data and (not isinstance(data[field], str) or not data[field].strip()):
                self.error(path, field + " must be a nonempty string")
        for field in ("hidden", "disable", "subtask"):
            if field in data and type(data[field]) is not bool:
                self.error(path, field + " must be boolean")
        if agent:
            if type(data.get("steps")) is not int or not 1 <= data["steps"] <= 1000:
                self.error(path, "steps must be an integer in 1..1000")
            for field, maximum in (("temperature", 2), ("top_p", 1)):
                if field in data and (type(data[field]) not in (int, float)
                                      or not 0 <= data[field] <= maximum):
                    self.error(path, field + " is out of range")
        return data

    def global_rule(self, path, rules, action, effect):
        if not any(r["action"] in (action, "*") and r["resource"] == "*"
                   and r["effect"] == effect for r in rules):
            self.error(path, "requires global {} {} rule".format(action, effect))
        if effect == "deny" and any(r["effect"] != "deny" and r["action"] in (action, "*") for r in rules):
            self.error(path, "{} must not have allow/ask exceptions".format(action))

    def restricted_agent(self, name, rules):
        path = AGENT_DIR + name + ".md"
        for action in ("read", "glob", "grep"):
            self.global_rule(path, rules, action, "allow")
        denied = ("edit", "shell", "subagent", "webfetch", "websearch") if name == "oxide-reviewer" else (
            "subagent", "webfetch", "websearch")
        for action in denied:
            self.global_rule(path, rules, action, "deny")
        if name == "oxide-reviewer":
            return
        edit_rules = [r for r in rules if r["action"] in ("edit", "*")]
        if not edit_rules or edit_rules[0]["resource"] != "*" or edit_rules[0]["effect"] != "deny":
            self.error(path, "edit requires default deny before scoped allows")
        permits = [r for r in edit_rules if r["effect"] != "deny"]
        expected = {"crates/*", "xtask/*", "tools/*"}
        if {r["resource"] for r in permits} != expected or any(r["effect"] != "allow" for r in permits):
            self.error(path, "edit allows must be exactly crates/*, xtask/*, tools/* after default deny")
        self.global_rule(path, rules, "shell", "allow")
        for pattern, sample in (("git*", "git status"), ("sudo*", "sudo true"),
                                ("rm -rf /*", "rm -rf /tmp/example")):
            if not any(r["effect"] == "deny" and r["resource"] in (pattern, "*")
                       for r in matching(rules, "shell", sample)):
                self.error(path, "requires shell deny " + pattern)
            if any(r["effect"] != "deny" and r["resource"] != "*"
                   for r in matching(rules, "shell", sample)):
                self.error(path, "shell allow/ask exception matches denied sample " + sample)
        if not any(r["resource"] == "tests/fixtures/**" and r["effect"] == "deny" for r in edit_rules):
            self.error(path, "requires explicit tests/fixtures/** edit deny")
        for sample in ("tests/fixtures/a.json", "tests/fixtures/nested/a.json", "docs/GOAL.md",
                       "opencode.json", "crates-escape/a.rs", "toolshed/a.py"):
            if any(r["effect"] != "deny" for r in matching(rules, "edit", sample)):
                self.error(path, "edit allow/ask unexpectedly matches protected sample " + sample)

    def references(self):
        # Resolve only known policy/agent/command filenames; do not match prose.
        known = set(self.texts)
        edges = {}
        for path, text in self.texts.items():
            edges[path] = {other for other in known if other != path and re.search(
                r"(?<![\w./~-])" + re.escape(other) + r"(?![\w/-]|\.[\w])", text)}
        for name, data in self.commands.items():
            target = AGENT_DIR + str(data.get("agent", "")) + ".md"
            if target in known:
                edges[COMMAND_DIR + name + ".md"].add(target)
        # The goal is source policy, not another consumer of the workflow.
        consumers = ["AGENTS.md", *[AGENT_DIR + n + ".md" for n in MODELS],
                     *[COMMAND_DIR + n + ".md" for n in ("oxide-kickoff", "oxide-continue")]]
        for path in consumers:
            if path not in known:
                continue
            reached, pending = set(), [path]
            while pending:
                current = pending.pop()
                if current not in reached:
                    reached.add(current)
                    pending.extend(edges.get(current, ()))
            for policy in POLICIES:
                if policy not in reached:
                    self.error(path, "missing required policy reference (direct or shared): " + policy)
        for path, text in self.texts.items():
            if re.search(r"(?:~/(?:[^\s`]+/)*|\.\./(?:[^\s`]+/)*)GOAL\.md\b", text):
                self.error(path, "active goal reference must be docs/GOAL.md, not a parent/home GOAL.md")

    def run(self):
        for path in POLICIES:
            self.text(path)
        self.global_config()
        toolchain = self.text("rust-toolchain.toml")
        if toolchain is not None:
            try:
                if tomllib.loads(toolchain).get("toolchain", {}).get("channel") != "1.99.1":
                    self.error("rust-toolchain.toml", "channel must be exactly 1.99.1")
            except (tomllib.TOMLDecodeError, AttributeError) as exc:
                self.error("rust-toolchain.toml", "invalid toolchain TOML: " + str(exc))
        for name, model in MODELS.items():
            path = AGENT_DIR + name + ".md"
            data = self.markdown(path, agent=True)
            if data is None:
                continue
            self.agents[name] = data
            mode = "primary" if name.startswith("oxide-orchestrator") else "subagent"
            if data.get("model") != model or data.get("mode") != mode:
                self.error(path, "model/mode must remain {} / {}".format(model, mode))
            rules = self.permissions(path, data)
            if mode == "subagent":
                self.restricted_agent(name, rules)
        for name in ("oxide-kickoff", "oxide-continue"):
            path = COMMAND_DIR + name + ".md"
            data = self.markdown(path, agent=False)
            if data is not None:
                self.commands[name] = data
                if data.get("agent") not in ("oxide-orchestrator", "oxide-orchestrator-deep", "oxide-orchestrator-argon"):
                    self.error(path, "command must select a known primary orchestrator")
                if "model" in data and data["model"] != MODELS.get(str(data.get("agent"))):
                    self.error(path, "command model must not override the primary model")
        self.references()
        return self.errors


def lint(root):
    return Linter(root, yaml_loader()).run()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    print(NOTICE)
    try:
        errors = lint(args.root.resolve())
    except LintFailure as exc:
        errors = [str(exc)]
    for error in errors:
        print("ERROR: " + error)
    print("workflow lint: {} ({} errors)".format("FAIL" if errors else "PASS", len(errors)))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
