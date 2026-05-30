#!/usr/bin/env python3
"""
qsafe 정적 audit 스크립트 — R51~R53 에서 발견된 false positive 들을 수정한 정확한 버전.

검사 차원:
  1. i18n parity (locale 간 키 일관)
  2. HTML/JS 참조 키 모두 정의됨
  3. getElementById 호출 ID ⊆ markup id
  4. querySelector # ID ⊆ markup id
  5. querySelector input[name="X"] X ⊆ markup name
  6. invoke('X') ⊆ tauri::command 등록
  7. typeof X 함수가 어딘가에 정의됨
  8. listen('X') ↔ backend emit (spawn_with_progress 동적 인자 포함)
  9. version alignment (Cargo.toml + tauri.conf.json)
 10. Tauri command body 안 .unwrap()/.expect() 없음
 11. JSON valid (8 locales + tauri.conf)
 12. Cargo.lock crate version 일관

사용:
  python3 scripts/audit.py

종료 코드:
  0 = 0 finding
  1 = 1+ finding
"""
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(ROOT)


def main() -> int:
    issues: list[str] = []
    locales = ["ko", "en", "ja", "zh", "es", "fr", "de", "it"]

    # 1) i18n parity
    data = {
        l: set(
            json.load(open(f"crates/qsafe-gui/ui/locales/{l}.json")).keys()
        )
        - {"_meta"}
        for l in locales
    }
    base = data["ko"] & data["en"]
    for l in locales:
        miss = base - data[l]
        if miss:
            issues.append(f"[i18n parity] {l} missing {len(miss)} keys")

    # 2) HTML/JS i18n refs all defined
    with open("crates/qsafe-gui/ui/index.html") as f:
        src = f.read()
    refs = set(re.findall(r'data-i18n(?:-title|-placeholder)?="([^"]+)"', src))
    refs |= set(re.findall(r'qsafeI18n\.t(?:Err)?\("([^"]+)"', src))
    undef = refs - data["en"] - data["ko"]
    if undef:
        issues.append(f"[i18n refs] undefined: {sorted(undef)}")

    # 3) getElementById ID ⊆ markup id
    ids_called = set(re.findall(r'getElementById\("([^"]+)"\)', src))
    ids_def = set(re.findall(r'id="([^"]+)"', src))
    miss = ids_called - ids_def
    if miss:
        issues.append(f"[getElementById] missing markup id: {sorted(miss)}")

    # 4) querySelector # ID
    qs_id = set(re.findall(r'querySelector(?:All)?\("\s*#([\w-]+)', src))
    qs_miss = qs_id - ids_def
    if qs_miss:
        issues.append(f"[querySelector #id] missing: {sorted(qs_miss)}")

    # 5) querySelector input[name="X"] X ⊆ name=
    qs_name = set(
        re.findall(r'querySelector(?:All)?\("input\[name="([\w-]+)"\]', src)
    )
    input_names = set(re.findall(r'name="([\w-]+)"', src))
    name_miss = qs_name - input_names
    if name_miss:
        issues.append(f"[querySelector name] missing: {sorted(name_miss)}")

    # 6) invoke ⊆ tauri::command
    with open("crates/qsafe-gui/src/main.rs") as f:
        main_rs = f.read()
    invokes = {
        x for x in re.findall(r'invoke\("([^"]+)"', src) if "|" not in x and ":" not in x
    }
    gen = re.search(
        r"tauri::generate_handler!\s*\[([^\]]+)\]", main_rs, re.DOTALL
    )
    reg = set(re.findall(r"commands::(\w+)", gen.group(1))) if gen else set()
    inv_miss = invokes - reg
    if inv_miss:
        issues.append(f"[invoke] not registered: {sorted(inv_miss)}")

    # 7) typeof X 함수가 어딘가에 정의됨
    typeofs = set(re.findall(r'typeof\s+(\w+)\s*===?\s*"function"', src))
    for name in typeofs:
        patterns = [
            rf"\bfunction\s+{name}\s*\(",
            rf"\b{name}\s*=\s*function\b",
            rf"\bconst\s+{name}\s*=",
            rf"\blet\s+{name}\s*=",
            rf"\bvar\s+{name}\s*=",
            rf"\bwindow\.{name}\s*=",
        ]
        if not any(re.search(p, src) for p in patterns):
            issues.append(f"[typeof] {name}: not defined anywhere")

    # 8) listen ↔ emit (spawn_with_progress 의 event_name 인자 포함)
    with open("crates/qsafe-gui/src/commands.rs") as f:
        cmds_rs = f.read()
    emits: set[str] = set()
    # 직접 app.emit("name", ...)
    emits.update(re.findall(r'\.emit\(\s*"([^"]+)"', cmds_rs))
    # spawn_with_progress(..., "event_name") — 마지막 string 인자
    # 정확한 매칭: spawn_with_progress\([^"]*"([^"]+)"\s*\)
    for m in re.finditer(r"spawn_with_progress\(([^)]+)\)", cmds_rs):
        args = m.group(1)
        strings = re.findall(r'"([^"]+)"', args)
        if strings:
            emits.add(strings[-1])  # 마지막 string = event_name
    listens = set(re.findall(r'\.listen\("([^"]+)"', src))
    builtins = {"tauri://drag-drop", "drop"}
    not_emitted = listens - emits - builtins
    if not_emitted:
        issues.append(f"[listen ↔ emit] never emitted: {sorted(not_emitted)}")

    # 9) version alignment
    v_cargo = re.search(
        r'^version\s*=\s*"([^"]+)"', open("Cargo.toml").read(), re.M
    ).group(1)
    v_tauri = json.load(open("crates/qsafe-gui/tauri.conf.json"))["version"]
    if v_cargo != v_tauri:
        issues.append(f"[version] drift Cargo={v_cargo} Tauri={v_tauri}")

    # 10) Tauri command body 안 .unwrap()/.expect() 없음
    # 정규식: #[tauri::command] 다음에 다른 attribute 들 (0+ 개) 있을 수 있음
    # pattern: #[tauri::command]\s*(?:#\[[^\]]+\]\s*)*pub fn (\w+)
    fn_blocks = re.finditer(
        r"#\[tauri::command\]\s*(?:(?:#\[[^\]]+\][^\n]*|//[^\n]*)\s*)*pub fn (\w+)",
        cmds_rs,
    )
    unwrap_hits = []
    for m in fn_blocks:
        fname = m.group(1)
        # body 찾기 — 다음 { 부터 매칭되는 }
        rest = cmds_rs[m.end():]
        open_idx = rest.find("{")
        if open_idx < 0:
            continue
        depth, end = 0, -1
        for j in range(open_idx, len(rest)):
            if rest[j] == "{":
                depth += 1
            elif rest[j] == "}":
                depth -= 1
                if depth == 0:
                    end = j
                    break
        if end < 0:
            continue
        body = rest[open_idx:end]
        if re.search(r"\.unwrap\(\)|\.expect\(", body):
            unwrap_hits.append(fname)
    if unwrap_hits:
        issues.append(f"[Tauri cmd panic-prone] .unwrap/.expect: {unwrap_hits}")

    # 11) JSON valid
    for f in [
        "crates/qsafe-gui/tauri.conf.json",
    ] + [f"crates/qsafe-gui/ui/locales/{l}.json" for l in locales]:
        try:
            json.load(open(f))
        except Exception as e:
            issues.append(f"[JSON invalid] {f}: {e}")

    # 12) Cargo.lock workspace crate 버전 일관
    with open("Cargo.lock") as f:
        lock = f.read()
    member_crates = [
        "qsafe-core",
        "qsafe-gui",
        "qsafe-cli",
        "qsafe-stub",
        "qsafe-crypto",
        "qsafe-formats",
        "qsafe-identity",
        "qsafe-paper",
        "qsafe-shamir",
        "qsafe-hardware",
    ]
    for crate in member_crates:
        for v in re.findall(
            rf'\[\[package\]\]\nname = "{crate}"\nversion = "([^"]+)"', lock
        ):
            if v != v_cargo:
                issues.append(f"[Cargo.lock] {crate}={v} != workspace={v_cargo}")

    # 13) Tauri command def vs registered
    cmd_defs = set(
        re.findall(
            r"#\[tauri::command\]\s*(?:(?:#\[[^\]]+\][^\n]*|//[^\n]*)\s*)*pub fn (\w+)",
            cmds_rs,
        )
    )
    unreg = cmd_defs - reg
    if unreg:
        issues.append(f"[Tauri cmd unregistered] defined but not in handler: {sorted(unreg)}")
    unknown = reg - cmd_defs
    if unknown:
        issues.append(f"[Tauri cmd no impl] registered but no #[tauri::command]: {sorted(unknown)}")

    # 결과
    print(f"\nqsafe static audit — {len(issues)} issues found\n")
    for i in issues:
        print(f"  ⚠ {i}")
    if not issues:
        print("  ✓ all dimensions clean")
    print()
    return 1 if issues else 0


if __name__ == "__main__":
    sys.exit(main())
