#!/usr/bin/env python3
"""Generate src-tauri/src/domain/appstore/catalog_meta.rs from a 1Panel-dev/appstore checkout.

Usage:
    python scripts/gen-appstore-meta.py <path-to-appstore-repo>
    # repo layout expected: <path>/apps/<key>/data.yml

Sources: apps/*/data.yml 顶层 name / description / tags；description 缺失时
回退 additionalProperties.shortDescZh 与 title；分类取第一个命中官方分类集的 tag。
"""
import json
import os
import re
import sys

CATEGORIES = [
    "AI", "建站", "数据库", "Web 服务器", "运行环境", "实用工具", "云存储",
    "BI", "CRM", "安全", "开发工具", "DevOps", "中间件", "多媒体", "邮件服务",
    "休闲游戏", "本地",
]


def rust_escape(value: str) -> str:
    """json.dumps 保证合法 Rust 字符串字面量的转义。"""
    out = json.dumps(value, ensure_ascii=False)
    out = re.sub(r"\\u([0-9a-fA-F]{4})", lambda m: "\\u{" + m.group(1) + "}", out)
    return out


def main() -> int:
    """读取官方应用目录并生成客户端内置的 Rust 元数据表。"""
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    root = sys.argv[1]
    import yaml  # PyYAML; install with: python -m pip install pyyaml

    rows = []
    apps_dir = os.path.join(root, "apps")
    for key in sorted(os.listdir(apps_dir)):
        data_yml = os.path.join(apps_dir, key, "data.yml")
        if not os.path.isfile(data_yml):
            continue
        with open(data_yml, encoding="utf-8") as handle:
            doc = yaml.safe_load(handle) or {}
        name = (doc.get("name") or "").strip() or key
        description = (doc.get("description") or "").strip()
        if not description:
            additional = doc.get("additionalProperties") or {}
            description = (additional.get("shortDescZh") or "").strip()
        if not description:
            description = (doc.get("title") or "").strip()
        tags = doc.get("tags") or []
        category = next((t for t in tags if t in CATEGORIES), "其他")
        rows.append(
            {"key": key, "name": name, "desc": description, "cat": category}
        )

    lines = [
        "//! 由 scripts/gen-appstore-meta.py 生成：全部应用官方元数据 (key/name/description/category)。",
        "//! 来源：1Panel-dev/appstore dev 分支 apps/*/data.yml（name、description、tags）。",
        "//! 重新生成：python scripts/gen-appstore-meta.py <appstore 源码目录>",
        "",
        "#[derive(Debug, Clone, Copy)]",
        "pub struct CatalogMeta {",
        "    pub key: &'static str,",
        "    pub name: &'static str,",
        "    pub description: &'static str,",
        "    pub category: &'static str,",
        "}",
        "",
        "/// 按 key 查找官方元数据；未命中返回 None（调用方使用兜底文案）。",
        "pub fn for_key(key: &str) -> Option<&'static CatalogMeta> {",
        "    CATALOG_META.iter().find(|meta| meta.key == key)",
        "}",
        "",
        "const CATALOG_META: &[CatalogMeta] = &[",
    ]
    for row in rows:
        lines.append(
            "    CatalogMeta {{ key: {}, name: {}, description: {}, category: {} }},".format(
                rust_escape(row["key"]),
                rust_escape(row["name"]),
                rust_escape(row["desc"]),
                rust_escape(row["cat"]),
            )
        )
    lines.append("];")
    lines.append("")

    destination = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "src-tauri",
        "src",
        "domain",
        "appstore",
        "catalog_meta.rs",
    )
    with open(destination, "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines))
    print(f"wrote {len(rows)} entries -> {destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
