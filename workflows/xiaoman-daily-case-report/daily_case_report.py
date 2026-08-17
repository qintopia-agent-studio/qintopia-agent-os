"""Xiaoman daily community case-file report — thin CLI entry point.

Deterministic report building lives in report_builder.py, rendering in
renderer.py, data models in models.py, message collection in collector.py and
analysis in analyzer.py.  This module keeps only argument parsing, production
boundary checks, artifact/result assembly and the main() orchestration, and
re-exports the refactored symbols so existing imports keep working.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Any
from zoneinfo import ZoneInfo

# Workflow directory for sibling imports (report_builder, renderer, etc.)
_WORKFLOW_DIR = Path(__file__).resolve().parent
if str(_WORKFLOW_DIR) not in sys.path:
    sys.path.insert(0, str(_WORKFLOW_DIR))

# Refactored modules
from models import (
    CHAT_ID_ENV,
    DEFAULT_GROUP_NAME,
    DEFAULT_IMAGE_FORMAT,
    DEFAULT_JPEG_QUALITY,
    DEFAULT_OUTPUT_WIDTH,
    DEFAULT_REPORT_TITLE,
    DEFAULT_TEMPLATE,
    DEFAULT_TIMEZONE,
    NEWSPAPER_ELEGANT_TEMPLATE,
    ROAST_LONG_IMAGE_TEMPLATE,
    TEMPLATE_VERSION,
    CaseCard,
    CharacterCard,
    CharacterMemory,
    CreativeProfileMemory,
    HotTopic,
    ReportData,
    ReportMessage,
    Suspect,
    clean_text,
)
from collector import (
    _character_memory_from_rows,
    _creative_profile_memory_from_rows,
    _fetch_messages_with_psql,
    database_url,
    fetch_character_memory,
    fetch_creative_profile_memory,
    fetch_messages,
    load_fixture,
    require_read_through,
)
from analyzer import (
    _detect_topic_markers,
    case_storyline_label,
    cluster_cases,
    compute_characters,
    compute_suspects,
    discussion_messages,
    extract_highlight,
    hot_topics,
    hourly_timeline,
    node_key,
    profile_upgrade_reason,
    profile_upgrade_status,
)
from report_builder import (
    _build_character_universe,
    _build_creative_profile_review_payload_draft,
    _build_draft_bundle,
    _build_quote_map,
    _build_report,
    _build_run_manifest,
    _build_wiki_bundle,
    _character_universe_summary,
    _daily_opening_line,
    _lookback_callback_candidates,
    _main_storyline_label,
    _meme_callback_candidates,
    _normalize_message_times,
    _ordinary_digest_candidate_topics,
    _ordinary_digest_local_life_notes,
    _ordinary_digest_open_questions,
    _ordinary_digest_people_notes,
    _ordinary_digest_topic_cards,
    _public_output_style_contract,
    _quote_entry,
    _relationship_candidates,
    _render_review_report,
    _report_date,
    _report_date_at,
    _report_timezone,
    _sample_messages,
    _source_chat_ref,
    _summary_result_json,
    _time_range_label,
    _wiki_bundle_counts,
)
from renderer import (
    _bar_svg,
    _build_newspaper_elegant_input,
    _draw_card,
    _draw_wrapped_text,
    _file_url,
    _font_candidates,
    _image_extension,
    _image_mime_type,
    _pil_font,
    _render_daily_markdown,
    _render_html,
    _render_image,
    _render_image_with_pillow,
    _render_image_with_playwright,
    _render_newspaper_html,
    _render_png,
    _render_v3_html,
    _wrap_for_draw,
)



def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Xiaoman daily community case-file report")
    parser.add_argument(
        "--date",
        help="Backfill one calendar day (YYYY-MM-DD). Omit for the most recently completed calendar day (00:00-24:00).",
    )
    parser.add_argument(
        "--chat-id",
        default=os.environ.get(CHAT_ID_ENV),
        help=(
            "QiWe chat id to report on. Required for database mode; "
            f"may be set with {CHAT_ID_ENV}."
        ),
    )
    parser.add_argument("--group-name", default=DEFAULT_GROUP_NAME)
    parser.add_argument("--report-title", default=DEFAULT_REPORT_TITLE)
    parser.add_argument("--timezone", default=DEFAULT_TIMEZONE)
    parser.add_argument("--output-dir", default="/tmp/xiaoman-daily-case-report")
    parser.add_argument("--output-width", type=int, default=DEFAULT_OUTPUT_WIDTH)
    parser.add_argument("--fixture", help="Path to JSON fixture with pre-canned messages.")
    parser.add_argument(
        "--render",
        choices=["auto", "image", "png", "html"],
        default="image",
        help="Render mode. image produces the poster; png is a legacy alias; html keeps the raw page for debugging.",
    )
    parser.add_argument(
        "--template",
        choices=["roast-long-image", "newspaper-elegant", "newspaper", "v3"],
        default=os.environ.get("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TEMPLATE", DEFAULT_TEMPLATE),
        help=(
            "Poster template. roast-long-image renders the LLM narrative as a "
            "long-form article JPEG (default, requires --narrative roast); "
            "newspaper-elegant is the wx-cli inspired broadsheet; newspaper is "
            "the compact editorial variant; v3 keeps the legacy vertical scoreboard."
        ),
    )
    parser.add_argument(
        "--image-format",
        choices=["jpeg", "png"],
        default=None,
        help="Image encoding for rendered output. Defaults to png for --render png, otherwise jpeg.",
    )
    parser.add_argument(
        "--keep-html",
        action="store_true",
        help="Keep the intermediate HTML file (debug only; the image is the deliverable).",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print full JSON instead of just the operator review message.",
    )
    parser.add_argument(
        "--json-summary-only",
        action="store_true",
        help="With --json, omit private rendered bodies and keep only paths, counts, and flags.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Generate from fixture or empty stub; do not read the database.",
    )
    parser.add_argument(
        "--narrative",
        choices=["none", "roast", "normal"],
        default="none",
        help=(
            "Optional LLM narrative layer. 'roast' / 'normal' ask an OpenAI-compatible "
            "endpoint to retell the deterministic report as a story. 'none' (default) "
            "keeps the pipeline fully deterministic and zero-LLM."
        ),
    )
    parser.add_argument(
        "--narrative-with-images",
        action="store_true",
        help=(
            "Allow the narrative to embed reviewed images. Requires --reviewed-image-dir. "
            "Gated by privacy boundary: attachments are not read by default."
        ),
    )
    parser.add_argument(
        "--reviewed-image-dir",
        default=None,
        help="Directory of reviewed, safe-to-publish images for narrative embedding.",
    )
    parser.add_argument("--llm-base-url", default=None, help="Override LLM base URL.")
    parser.add_argument("--llm-api-key", default=None, help="Override LLM API key.")
    parser.add_argument("--llm-model", default=None, help="Override LLM model name.")
    args = parser.parse_args()
    _normalize_render_args(args)
    return args


def _normalize_render_args(args: argparse.Namespace) -> None:
    if args.image_format is None:
        args.image_format = "png" if args.render == "png" else DEFAULT_IMAGE_FORMAT
    template = getattr(args, "template", DEFAULT_TEMPLATE)
    narrative = getattr(args, "narrative", "none")
    args.template = template
    args.narrative = narrative
    if template in ("newspaper", NEWSPAPER_ELEGANT_TEMPLATE, ROAST_LONG_IMAGE_TEMPLATE) and args.output_width == DEFAULT_OUTPUT_WIDTH:
        args.output_width = 1080
    if template == ROAST_LONG_IMAGE_TEMPLATE and narrative == "none":
        args.narrative = "roast"


def _uses_real_messages(args: argparse.Namespace) -> bool:
    return not args.dry_run and not args.fixture


def _validate_production_boundaries(args: argparse.Namespace) -> None:
    if not args.chat_id:
        raise RuntimeError(
            f"production read-through requires --chat-id or {CHAT_ID_ENV}; "
            "do not run an unscoped group-message query"
        )


def _prepare_output_dir(path: str) -> Path:
    output_dir = Path(path)
    existed = output_dir.exists()
    output_dir.mkdir(parents=True, exist_ok=True)
    if existed:
        mode = output_dir.stat().st_mode & 0o777
        if mode != 0o700:
            raise RuntimeError(
                f"output directory already exists with mode {mode:04o}; "
                "use a dedicated private 0700 directory"
            )
    else:
        output_dir.chmod(0o700)
    return output_dir


def _write_private_text(path: Path, content: str) -> None:
    with tempfile.NamedTemporaryFile(
        "w",
        encoding="utf-8",
        dir=path.parent,
        prefix=f".{path.stem}-",
        suffix=path.suffix,
        delete=False,
    ) as handle:
        tmp_path = Path(handle.name)
        os.chmod(tmp_path, 0o600)
        handle.write(content)
    tmp_path.replace(path)
    os.chmod(path, 0o600)



def _artifact_candidate(
    path: Path,
    image_format: str,
    report: ReportData,
    output_width: int | None = None,
    source_chat_id: str | None = None,
    template: str = DEFAULT_TEMPLATE,
) -> dict[str, Any]:
    data = path.read_bytes()
    return {
        "artifact_type": "generated_image",
        "workflow_type": "daily_case_report",
        "template_version": (
            "xiaoman-daily-case-report-v3" if template == "v3"
            else "xiaoman-daily-case-report-v4-newspaper" if template in (NEWSPAPER_ELEGANT_TEMPLATE, "newspaper")
            else TEMPLATE_VERSION
        ),
        "mime_type": _image_mime_type(image_format),
        "filename": path.name,
        "content_hash": f"sha256:{hashlib.sha256(data).hexdigest()}",
        "file_md5": hashlib.md5(data).hexdigest(),  # nosec: QiWe protocol requires MD5.
        "byte_size": len(data),
        "render": {
            "image_format": image_format,
            "width": output_width,
            "jpeg_quality": DEFAULT_JPEG_QUALITY if image_format == "jpeg" else None,
        },
        "report_window": {
            "start": report.window_start,
            "end": report.window_end,
            "display": report.report_date,
            "time_range": report.time_range,
            "timezone": report.timezone,
        },
        "content_metrics": {
            "message_count": report.message_count,
            "participant_count": report.participant_count,
            "case_count": report.case_count,
            "character_count": report.character_count,
            "hot_topic_count": len(report.hot_topics),
        },
        "source_chat_ref": _source_chat_ref(source_chat_id),
        "retained_source_policy": "sanitized_metadata_only",
    }


def _operator_review_message(
    report: ReportData,
    html_path: Path,
    image_path: Path | None,
    include_html: bool = False,
) -> str:
    lines = [
        f"【{report.group_name}｜小满群聊日报】",
        f"日报日期：{report.report_date}（{report.time_range}）",
        f"消息 {report.message_count} 条 / 活跃 {report.participant_count} 人 / 主线 {report.case_count} 条 / 剧中人 {report.character_count} 位 / 发言榜 {report.suspect_count} 名",
        f"今日主线：{_main_storyline_label(report)}",
        "",
    ]
    for case in report.cases:
        lines.append(f"• 主线 {case.case_no.replace('CASE ', '')}：{case_storyline_label(case)}（{case.summary}）")
    for character in report.characters:
        lines.append(f"• 剧中人 {character.rank}：{character.name}｜{character.role_label}｜{character.story_function}")
    lines.append("")
    if image_path:
        lines.append(f"图片文件：{image_path}")
    if include_html and html_path.exists():
        label = "HTML 预览（仅调试用）" if image_path else "HTML 预览"
        lines.append(f"{label}：{html_path}")
    lines.append("")
    lines.append("本报告仅生成本地文件，尚未自动发布；生产自动发布需接入 AgentOS artifact 与 QiWe image-send。")
    return "\n".join(lines)


def _result_json(
    report: ReportData,
    deliverable_path: Path,
    image_path: Path | None,
    image_format: str | None = None,
    html_path: Path | None = None,
    markdown_path: Path | None = None,
    universe_path: Path | None = None,
    quote_map_path: Path | None = None,
    wiki_bundle_path: Path | None = None,
    draft_bundle_path: Path | None = None,
    run_manifest_path: Path | None = None,
    review_report_path: Path | None = None,
    creative_profile_review_payload_path: Path | None = None,
    quote_map: dict[str, Any] | None = None,
    wiki_bundle: dict[str, Any] | None = None,
    draft_bundle: dict[str, Any] | None = None,
    run_manifest: dict[str, Any] | None = None,
    creative_profile_review_payload: dict[str, Any] | None = None,
    output_width: int | None = None,
    source_chat_id: str | None = None,
    template: str = DEFAULT_TEMPLATE,
) -> dict[str, Any]:
    html_exists = html_path is not None and html_path.exists()
    markdown_exists = markdown_path is not None and markdown_path.exists()
    universe_exists = universe_path is not None and universe_path.exists()
    quote_map_exists = quote_map_path is not None and quote_map_path.exists()
    wiki_bundle_exists = wiki_bundle_path is not None and wiki_bundle_path.exists()
    draft_bundle_exists = draft_bundle_path is not None and draft_bundle_path.exists()
    run_manifest_exists = run_manifest_path is not None and run_manifest_path.exists()
    review_report_exists = review_report_path is not None and review_report_path.exists()
    creative_profile_review_payload_exists = (
        creative_profile_review_payload_path is not None
        and creative_profile_review_payload_path.exists()
    )
    quote_map = quote_map or _build_quote_map(report)
    wiki_bundle = wiki_bundle or _build_wiki_bundle(report, quote_map)
    draft_bundle = draft_bundle or _build_draft_bundle(report, quote_map, wiki_bundle)
    run_manifest = run_manifest or _build_run_manifest(
        report,
        quote_map,
        wiki_bundle,
        draft_bundle,
        source_chat_id=source_chat_id,
    )
    creative_profile_review_payload = creative_profile_review_payload or {}
    artifact_candidate = (
        _artifact_candidate(image_path, image_format, report, output_width, source_chat_id, template)
        if image_path is not None and image_format is not None and image_path.exists()
        else None
    )
    return {
        "success": True,
        "skill": "xiaoman_daily_case_report",
        "external_send_executed": False,
        "requires_human_confirmation": False,
        "auto_publish_ready": False,
        "group_name": report.group_name,
        "report_date": report.report_date,
        "time_range": report.time_range,
        "message_count": report.message_count,
        "participant_count": report.participant_count,
        "case_count": report.case_count,
        "character_count": report.character_count,
        "suspect_count": report.suspect_count,
        "deliverable_path": str(deliverable_path),
        "image_path": str(image_path) if image_path else None,
        "image_format": image_format,
        "image_mime_type": _image_mime_type(image_format) if image_format else None,
        "png_path": str(image_path) if image_format == "png" and image_path else None,
        "html_path": str(html_path) if html_exists else None,
        "daily_report_markdown_path": str(markdown_path) if markdown_exists else None,
        "daily_report_markdown": _render_daily_markdown(report),
        "public_output_style": _public_output_style_contract(),
        "character_universe_path": str(universe_path) if universe_exists else None,
        "character_universe": report.character_universe,
        "character_universe_summary": _character_universe_summary(report.character_universe),
        "quote_map_path": str(quote_map_path) if quote_map_exists else None,
        "quote_map": quote_map,
        "wiki_bundle_path": str(wiki_bundle_path) if wiki_bundle_exists else None,
        "wiki_bundle": wiki_bundle,
        "draft_bundle_path": str(draft_bundle_path) if draft_bundle_exists else None,
        "draft_bundle": draft_bundle,
        "run_manifest_path": str(run_manifest_path) if run_manifest_exists else None,
        "run_manifest": run_manifest,
        "review_report_path": str(review_report_path) if review_report_exists else None,
        "creative_profile_review_payload_path": (
            str(creative_profile_review_payload_path)
            if creative_profile_review_payload_exists
            else None
        ),
        "private_review_bundle": {
            "schema_version": "xiaoman-daily-private-review-bundle-v1",
            "source": "wx_cli_style_daily_migration",
            "public_surface_allowed": False,
            "review_required": True,
            "raw_message_rows_included": False,
            "profile_fact_text_included": False,
            "raw_message_payload_read": (run_manifest.get("privacy") or {}).get(
                "raw_message_payload_read"
            )
            is True,
            "attachment_public_surface_allowed": (run_manifest.get("privacy") or {}).get(
                "attachment_public_surface_allowed"
            )
            is True,
            "quote_map_entry_count": quote_map.get("entry_count", 0),
            "wiki_counts": wiki_bundle.get("counts", {}),
            "draft_counts": draft_bundle.get("counts", {}),
            "run_manifest_schema_version": run_manifest.get("schema_version", ""),
            "creative_profile_review_payload": {
                "schema_version": creative_profile_review_payload.get("schema_version"),
                "source": creative_profile_review_payload.get("source", ""),
                "candidate_count": len(creative_profile_review_payload.get("candidates") or []),
                "pending_review_count": sum(
                    1
                    for candidate in creative_profile_review_payload.get("candidates") or []
                    if candidate.get("review_decision") == "pending_review"
                ),
                "approved_candidate_count": sum(
                    1
                    for candidate in creative_profile_review_payload.get("candidates") or []
                    if candidate.get("review_decision") == "approved"
                ),
                "person_id_required": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("person_id_required")
                is True,
                "display_name_binding_allowed": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("display_name_binding_allowed")
                is True,
                "public_surface_allowed": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("public_surface_allowed")
                is True,
                "raw_messages_included": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("raw_messages_included")
                is True,
                "profile_fact_text_included": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("profile_fact_text_included")
                is True,
            },
        },
        "artifact_candidate": artifact_candidate,
        "operator_review_message": _operator_review_message(
            report, html_path or deliverable_path, image_path, html_exists
        ),
    }


def main() -> int:
    args = _parse_args()
    _normalize_render_args(args)

    real_messages = _uses_real_messages(args)
    if real_messages and (args.keep_html or args.render == "html"):
        print(
            "ERROR: production read-through cannot retain HTML because it contains real group content; "
            "use --render image without --keep-html",
            file=sys.stderr,
        )
        return 2
    if real_messages:
        try:
            _validate_production_boundaries(args)
        except RuntimeError as exc:
            print(f"ERROR: {exc}", file=sys.stderr)
            return 2

    try:
        report = _build_report(args)
    except RuntimeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    # Optional LLM narrative layer. Kept fail-safe: an LLM/config error must
    # never break the deterministic pipeline.
    narrative_md = None
    if args.narrative != "none":
        try:
            from narrative_generator import NarrativeConfig, generate_narrative
            cfg = NarrativeConfig.from_env(
                base_url=getattr(args, "llm_base_url", None),
                api_key=getattr(args, "llm_api_key", None),
                model=getattr(args, "llm_model", None),
            )
            reviewed_dir = (
                getattr(args, "reviewed_image_dir", None)
                if getattr(args, "narrative_with_images", False)
                else None
            )
            narrative_md = generate_narrative(args.narrative, report, cfg, reviewed_dir)
        except Exception as exc:  # noqa: BLE001 - narrative is best-effort
            print(f"WARN: narrative generation skipped: {exc}", file=sys.stderr)

    output_dir = _prepare_output_dir(args.output_dir)
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    html_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.html"
    markdown_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.md"
    universe_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.character-universe.json"
    quote_map_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.quote-map.json"
    wiki_bundle_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.wiki-bundle.json"
    draft_bundle_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.draft-bundle.json"
    run_manifest_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.run-manifest.json"
    review_report_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.review.md"
    narrative_path = (
        output_dir / f"xiaoman-daily-case-report-{timestamp}.{args.narrative}.md"
        if args.narrative != "none"
        else None
    )
    creative_profile_review_payload_path = (
        output_dir
        / f"xiaoman-daily-case-report-{timestamp}.creative-profile-review-payload.draft.json"
    )
    image_path = output_dir / (
        f"xiaoman-daily-case-report-{timestamp}.{_image_extension(args.image_format)}"
    )

    # Write the narrative artifact first so a later render hiccup can never drop it.
    if narrative_path is not None and narrative_md:
        _write_private_text(narrative_path, narrative_md)

    html_content = _render_html(report, args.output_width, args.template, narrative_md)
    quote_map = _build_quote_map(report)
    wiki_bundle = _build_wiki_bundle(report, quote_map)
    draft_bundle = _build_draft_bundle(report, quote_map, wiki_bundle)
    run_manifest = _build_run_manifest(
        report,
        quote_map,
        wiki_bundle,
        draft_bundle,
        source_chat_id=args.chat_id,
    )
    creative_profile_review_payload = _build_creative_profile_review_payload_draft(
        report.character_universe,
        datetime.now(_report_timezone(getattr(args, "timezone", DEFAULT_TIMEZONE)))
        .replace(microsecond=0)
        .isoformat(),
    )
    _write_private_text(html_path, html_content)
    _write_private_text(markdown_path, _render_daily_markdown(report))
    _write_private_text(
        universe_path,
        json.dumps(report.character_universe, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        quote_map_path,
        json.dumps(quote_map, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        wiki_bundle_path,
        json.dumps(wiki_bundle, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        draft_bundle_path,
        json.dumps(draft_bundle, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        run_manifest_path,
        json.dumps(run_manifest, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        review_report_path,
        _render_review_report(report, quote_map, wiki_bundle, draft_bundle, run_manifest),
    )
    _write_private_text(
        creative_profile_review_payload_path,
        json.dumps(creative_profile_review_payload, ensure_ascii=False, indent=2, sort_keys=True),
    )

    image_generated = False
    try:
        if args.render in ("auto", "image", "png"):
            try:
                _render_image(
                    html_path,
                    image_path,
                    args.output_width,
                    args.image_format,
                    report,
                )
                image_generated = True
            except RuntimeError as exc:
                print(f"WARN: image rendering skipped: {exc}", file=sys.stderr)
                if args.render in ("image", "png") or real_messages:
                    return 2

        html_is_deliverable = not image_generated

        deliverable = image_path if image_generated else html_path
        result = _result_json(
            report,
            deliverable,
            image_path if image_generated else None,
            args.image_format if image_generated else None,
            None if real_messages else html_path if html_path.exists() else None,
            markdown_path if markdown_path.exists() else None,
            universe_path if universe_path.exists() else None,
            quote_map_path if quote_map_path.exists() else None,
            wiki_bundle_path if wiki_bundle_path.exists() else None,
            draft_bundle_path if draft_bundle_path.exists() else None,
            run_manifest_path if run_manifest_path.exists() else None,
            review_report_path if review_report_path.exists() else None,
            creative_profile_review_payload_path
            if creative_profile_review_payload_path.exists()
            else None,
            quote_map,
            wiki_bundle,
            draft_bundle,
            run_manifest,
            creative_profile_review_payload,
            args.output_width if image_generated else None,
            args.chat_id if image_generated else None,
            args.template,
        )

        if args.json:
            output = (
                _summary_result_json(result)
                if getattr(args, "json_summary_only", False)
                else result
            )
            print(json.dumps(output, ensure_ascii=False, indent=2))
        else:
            print(result["operator_review_message"])
        return 0
    finally:
        html_is_deliverable = not image_generated
        should_remove_html = real_messages or (not args.keep_html and not html_is_deliverable)
        if should_remove_html and html_path.exists():
            try:
                html_path.unlink()
            except OSError:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
