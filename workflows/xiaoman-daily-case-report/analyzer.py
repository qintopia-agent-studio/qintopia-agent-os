"""Text analysis, topic clustering, and character detection for the daily report."""
from __future__ import annotations

import hashlib
import re
from collections import Counter
from datetime import datetime
from typing import Any

from models import (
    CASE_CARD_COLORS,
    CHARACTER_ROLE_RULES,
    CharacterCard,
    CharacterMemory,
    CreativeProfileMemory,
    DEFAULT_CASE_LIMIT,
    DEFAULT_CHARACTER_LIMIT,
    DEFAULT_HOURLY_BUCKETS,
    DEFAULT_HOT_TOPIC_LIMIT,
    DEFAULT_HOT_TOPIC_MAX_CHARS,
    DEFAULT_HOT_TOPIC_MIN_CHARS,
    DEFAULT_HOT_TOPIC_MIN_MESSAGES,
    DEFAULT_MIN_CASE_MESSAGES,
    DEFAULT_SUSPECT_LIMIT,
    DEFAULT_TOP_KEYWORDS,
    HIGHLIGHT_SIGNAL_WORDS,
    HotTopic,
    MEMORY_LOOKBACK_DAYS,
    PROMOTIONAL_NOISE_PHRASES,
    ReportMessage,
    STOP_WORDS,
    Suspect,
    TOPIC_MARKER_HINTS,
    CaseCard,
    clean_text,
    memory_callback_seed,
    memory_depth_label,
    memory_recurrence_label,
    memory_weight_label,
)


# ---------------------------------------------------------------------------
# Text cleaning / noise filtering
# ---------------------------------------------------------------------------

def _looks_promotional_noise(text: str) -> bool:
    raw = text or ""
    cleaned = clean_text(raw)
    compact = re.sub(r"\s+", "", cleaned)
    if any(phrase in compact for phrase in PROMOTIONAL_NOISE_PHRASES):
        return True
    if re.search(r"[A-Za-z0-9:/._-]{10,}", raw) and any(
        phrase in compact for phrase in ("付款", "订单", "复制", "打开", "宝贝")
    ):
        return True
    return False


def discussion_messages(messages: list[ReportMessage]) -> list[ReportMessage]:
    return [m for m in messages if not _looks_promotional_noise(m.text)]


# ---------------------------------------------------------------------------
# Tokenization
# ---------------------------------------------------------------------------

def _tokenize(text: str) -> list[str]:
    """Tokenize text for keyword clustering. Uses jieba when available."""
    text = clean_text(text).lower()
    try:
        import jieba

        tokens = list(jieba.lcut(text))
    except ImportError:
        # Fallback: extract 2-4 character Chinese chunks and English words.
        chinese = re.findall(r"[\u4e00-\u9fa5]{2,4}", text)
        english = re.findall(r"[a-z]{3,}", text)
        tokens = chinese + english
    filtered: list[str] = []
    for token in tokens:
        token = token.strip()
        if not token or token in STOP_WORDS or len(token) < 2:
            continue
        if token.isdigit():
            continue
        filtered.append(token)
    return filtered


def _keyword_scores(messages: list[ReportMessage]) -> Counter:
    counter: Counter = Counter()
    for msg in messages:
        for token in _tokenize(msg.text):
            counter[token] += 1
    return counter


# ---------------------------------------------------------------------------
# Topic / case helpers
# ---------------------------------------------------------------------------

def _is_clean_topic(kw: str) -> bool:
    """Reject noise tokens so case titles stay meaningful."""
    if not kw or kw in STOP_WORDS:
        return False
    if kw.lower() in {"none", "null", "nan", "true", "false"}:
        return False
    if any(noise in kw for noise in ("现在规定叫", "规定叫")):
        return False
    if "群里" in kw:
        return False
    if any(noise in kw for noise in ("哈哈", "嘿嘿", "呵呵", "嘻嘻", "呲牙", "啧啧")):
        return False
    if kw.endswith(("不", "吗", "么", "吧", "呢", "啊", "呀", "啦", "哦", "哈", "的", "了")):
        return False
    if len(kw) >= 3 and len(set(kw)) == 1:
        return False
    if not any("\u4e00" <= c <= "\u9fa5" for c in kw):
        return False
    return True


def _is_time_bucket_topic(topic: str) -> bool:
    return bool(re.match(r"^(早场|午后|晚场|夜场)(?:[ ·][^ ]+)?\s*\d{2}:00", topic))


def _time_bucket_title(hour: int, messages: list[ReportMessage]) -> str:
    if 5 <= hour < 12:
        period = "早场"
    elif 12 <= hour < 18:
        period = "午后"
    elif 18 <= hour < 23:
        period = "晚场"
    else:
        period = "夜场"
    for keyword, count in _keyword_scores(messages).most_common(DEFAULT_TOP_KEYWORDS):
        if count >= DEFAULT_MIN_CASE_MESSAGES and _is_clean_topic(keyword):
            return f"{period} · {keyword}"
    return f"{period} {hour:02d}:00 时段"


def _topic_marker_title(cleaned: str) -> str | None:
    pattern = re.compile(r"^([^：:\n]{2,30})[：:]\s*")
    match = pattern.match(cleaned)
    if not match:
        return None
    topic = match.group(1).strip()
    if not (
        4 <= len(topic) <= 24
        and not topic[-1].isdigit()
        and not topic.endswith(("，", ",", "、"))
        and _is_clean_topic(topic)
    ):
        return None
    if not any(hint in topic for hint in TOPIC_MARKER_HINTS):
        return None
    return topic


def _is_digest_snippet_text(text: str) -> bool:
    cleaned = clean_text(text)
    if len(cleaned) < 12:
        return False
    if _looks_promotional_noise(cleaned):
        return False
    if any(noise in cleaned for noise in ("现在规定叫", "呲牙", "哈哈", "嘿嘿", "呵呵", "嘻嘻", "啧啧")):
        return any(word in cleaned for word in HIGHLIGHT_SIGNAL_WORDS)
    if re.match(r"^[^：:\n]{2,30}[：:]\s*", cleaned) and _topic_marker_title(cleaned) is None:
        return False
    return True


def _time_bucket_bullet(
    time_label: str,
    message_count: int,
    participant_count: int,
) -> str:
    return f"{time_label}：{message_count} 条群消息，{participant_count} 人参与。"


def _hot_topic_phrases(text: str) -> set[str]:
    phrases: set[str] = set()
    for source in re.findall(r"[\u4e00-\u9fa5]+", clean_text(text)):
        max_length = min(len(source), DEFAULT_HOT_TOPIC_MAX_CHARS)
        for length in range(DEFAULT_HOT_TOPIC_MIN_CHARS, max_length + 1):
            for start in range(len(source) - length + 1):
                phrase = source[start:start + length]
                if _is_clean_topic(phrase):
                    phrases.add(phrase)
    return phrases


def _detect_topic_markers(messages: list[ReportMessage]) -> dict[str, list[ReportMessage]]:
    """Group messages under explicit topic markers like 'Topic：'."""
    clusters: dict[str, list[ReportMessage]] = {}
    current_topic: str | None = None
    for msg in messages:
        cleaned = clean_text(msg.text)
        if cleaned.startswith("#接龙"):
            body = cleaned[3:].strip()
            m = re.match(r"^([^\s，,0-9]{2,20})", body)
            title = m.group(1) if m else body[:12]
            current_topic = f"接龙 · {title}"
        else:
            has_colon_marker = re.match(r"^[^：:\n]{2,30}[：:]\s*", cleaned) is not None
            topic = _topic_marker_title(cleaned)
            if topic:
                current_topic = topic
            elif has_colon_marker:
                current_topic = None
        if current_topic:
            clusters.setdefault(current_topic, []).append(msg)
    return clusters


def case_storyline_label(case: CaseCard) -> str:
    label = case.title.replace("关于「", "").replace("」的讨论", "").strip()
    return label or case.title


# ---------------------------------------------------------------------------
# Case clustering
# ---------------------------------------------------------------------------

def cluster_cases(
    messages: list[ReportMessage],
    limit: int = DEFAULT_CASE_LIMIT,
) -> list[CaseCard]:
    """Group messages into topical case cards."""
    if not messages:
        return []

    clusters = _detect_topic_markers(messages)
    time_bucket_titles: set[str] = set()
    assigned_ids = {id(m) for cluster in clusters.values() for m in cluster}
    unassigned = [m for m in messages if id(m) not in assigned_ids]

    keyword_scores = _keyword_scores(unassigned)
    top_keywords = [
        kw
        for kw, count in keyword_scores.most_common(DEFAULT_TOP_KEYWORDS)
        if count >= DEFAULT_MIN_CASE_MESSAGES and _is_clean_topic(kw)
    ]

    for msg in unassigned:
        tokens = set(_tokenize(msg.text))
        best_keyword = ""
        best_score = 0
        for kw in top_keywords:
            if kw in tokens and keyword_scores[kw] > best_score:
                best_keyword = kw
                best_score = keyword_scores[kw]
        if not best_keyword:
            continue
        clusters.setdefault(f"关于「{best_keyword}」的讨论", []).append(msg)

    qualified_cluster_count = sum(
        1 for cluster in clusters.values() if len(cluster) >= DEFAULT_MIN_CASE_MESSAGES
    )
    if qualified_cluster_count < limit:
        assigned_ids = {id(m) for cluster in clusters.values() for m in cluster}
        buckets: dict[int, list[ReportMessage]] = {}
        for msg in messages:
            if id(msg) in assigned_ids or not msg.sent_at:
                continue
            buckets.setdefault(msg.sent_at.hour, []).append(msg)
        for hour, bucket in sorted(buckets.items(), key=lambda item: (-len(item[1]), item[0])):
            if len(bucket) < DEFAULT_MIN_CASE_MESSAGES:
                continue
            title = _time_bucket_title(hour, bucket)
            while title in clusters:
                title = f"{title} · {hour:02d}:00"
            clusters[title] = bucket
            time_bucket_titles.add(title)
            qualified_cluster_count += 1
            if qualified_cluster_count >= limit:
                break

    sorted_clusters = sorted(
        clusters.items(),
        key=lambda item: (-len(item[1]), -_keyword_scores(messages).get(item[0], 0)),
    )

    cases: list[CaseCard] = []
    for index, (keyword, cluster) in enumerate(sorted_clusters[:limit], start=1):
        if len(cluster) < DEFAULT_MIN_CASE_MESSAGES:
            continue
        times = [m.sent_at for m in cluster if m.sent_at]
        if times:
            start_t, end_t = min(times), max(times)
            if start_t.date() == end_t.date():
                time_label = f"{start_t.strftime('%H:%M')}–{end_t.strftime('%H:%M')}"
            else:
                time_label = f"{start_t.strftime('%m/%d %H:%M')}–{end_t.strftime('%m/%d %H:%M')}"
        else:
            time_label = "时间未知"
        participants = {m.sender_name for m in cluster}
        speaker_counts: Counter = Counter()
        for m in cluster:
            name = m.sender_name or "匿名"
            if name != "匿名":
                speaker_counts[name] += 1
        top_speaker = speaker_counts.most_common(1)[0][0] if speaker_counts else "群友"
        if keyword in time_bucket_titles:
            bullets = [
                _time_bucket_bullet(
                    time_label,
                    len(cluster),
                    len(participants),
                )
            ]
        else:
            representative_messages = [m for m in cluster if _is_digest_snippet_text(m.text)]
            if not representative_messages:
                representative_messages = [
                    m for m in cluster if clean_text(m.text) and not _looks_promotional_noise(m.text)
                ]
            sorted_by_length = sorted(
                representative_messages,
                key=lambda m: (
                    -len(m.text),
                    m.sent_at.timestamp() if m.sent_at else float("-inf"),
                ),
            )[:3]
            bullets = []
            for m in sorted_by_length:
                snippet = clean_text(m.text)[:70]
                if snippet and snippet not in bullets:
                    bullets.append(snippet)
            if not bullets:
                continue

        color_bg, color_text = CASE_CARD_COLORS[(index - 1) % len(CASE_CARD_COLORS)]
        cases.append(
            CaseCard(
                case_no=f"CASE {index:02d}",
                title=keyword,
                time_label=time_label,
                summary=f"{len(cluster)} 条消息，{len(participants)} 人参与",
                bullets=bullets,
                message_count=len(cluster),
                participant_count=len(participants),
                top_speaker=top_speaker,
                color_bg=color_bg,
                color_text=color_text,
            )
        )
    return cases


# ---------------------------------------------------------------------------
# Hot topics
# ---------------------------------------------------------------------------

def hot_topics(
    messages: list[ReportMessage],
    cases: list[CaseCard] | None = None,
    limit: int = DEFAULT_HOT_TOPIC_LIMIT,
) -> list[HotTopic]:
    grouped: dict[str, list[ReportMessage]] = {}
    repeated_phrases: dict[str, list[ReportMessage]] = {}
    case_topic_stats: dict[str, tuple[int, int]] = {}
    for message in messages:
        for token in set(_tokenize(message.text)):
            if _is_clean_topic(token) and len(token) >= DEFAULT_HOT_TOPIC_MIN_CHARS:
                grouped.setdefault(token, []).append(message)
        for phrase in _hot_topic_phrases(message.text):
            repeated_phrases.setdefault(phrase, []).append(message)

    for phrase, group in repeated_phrases.items():
        if len({clean_text(message.text) for message in group}) >= DEFAULT_HOT_TOPIC_MIN_MESSAGES:
            existing = grouped.setdefault(phrase, [])
            existing_ids = {message.id for message in existing}
            existing.extend(message for message in group if message.id not in existing_ids)

    for case in cases or []:
        topic = case_storyline_label(case)
        if (
            case.message_count >= DEFAULT_HOT_TOPIC_MIN_MESSAGES
            and _is_clean_topic(topic)
            and not _is_time_bucket_topic(topic)
        ):
            current_message_count, current_participant_count = case_topic_stats.get(topic, (0, 0))
            case_topic_stats[topic] = (
                max(current_message_count, case.message_count),
                max(current_participant_count, case.participant_count),
            )

    ranked = sorted(
        (
            (
                keyword,
                max(len(grouped.get(keyword, [])), case_topic_stats.get(keyword, (0, 0))[0]),
                max(
                    len(
                        {
                            message.sender_id or message.sender_name
                            for message in grouped.get(keyword, [])
                        }
                    ),
                    case_topic_stats.get(keyword, (0, 0))[1],
                ),
            )
            for keyword in set(grouped) | set(case_topic_stats)
            if max(len(grouped.get(keyword, [])), case_topic_stats.get(keyword, (0, 0))[0])
            >= DEFAULT_HOT_TOPIC_MIN_MESSAGES
        ),
        key=lambda item: (
            -(len(item[0]) * item[1]),
            -item[1],
            -item[2],
            -len(item[0]),
            item[0],
        ),
    )
    topics: list[HotTopic] = []
    for keyword, message_count, participant_count in ranked:
        if any(keyword in topic.keyword or topic.keyword in keyword for topic in topics):
            continue
        topics.append(
            HotTopic(
                rank=len(topics) + 1,
                keyword=keyword,
                message_count=message_count,
                participant_count=participant_count,
            )
        )
        if len(topics) == limit:
            break
    return topics


# ---------------------------------------------------------------------------
# Suspects (most active speakers)
# ---------------------------------------------------------------------------

def compute_suspects(messages: list[ReportMessage], limit: int = DEFAULT_SUSPECT_LIMIT) -> list[Suspect]:
    counts: Counter = Counter()
    words: Counter = Counter()
    for msg in messages:
        name = msg.sender_name or "匿名"
        counts[name] += 1
        words[name] += len(clean_text(msg.text))

    suspects = []
    avatars = ["🕵️", "🕵️‍♀️", "🥷", "🦹", "🧙"]
    for rank, (name, msg_count) in enumerate(counts.most_common(limit), start=1):
        suspects.append(
            Suspect(
                rank=rank,
                name=name,
                message_count=msg_count,
                word_count=words[name],
                avatar_emoji=avatars[(rank - 1) % len(avatars)],
            )
        )
    return suspects


# ---------------------------------------------------------------------------
# Character analysis
# ---------------------------------------------------------------------------

def _character_role(messages: list[ReportMessage]) -> tuple[str, str, int]:
    text = "\n".join(clean_text(message.text) for message in messages)
    best_label = "在场感选手"
    best_line = "用持续出现把当天话题接住"
    best_score = 0
    for _role, label, line, hints in CHARACTER_ROLE_RULES:
        score = sum(text.count(hint) for hint in hints)
        if score > best_score:
            best_label = label
            best_line = line
            best_score = score
    return best_label, best_line, best_score


def _character_evidence(messages: list[ReportMessage]) -> str:
    candidates: list[tuple[int, str]] = []
    for message in messages:
        text = clean_text(message.text)
        if not _is_digest_snippet_text(text):
            continue
        score = min(len(text), 90)
        if any(word in text for word in HIGHLIGHT_SIGNAL_WORDS):
            score += 20
        if any(hint in text for _role, _label, _line, hints in CHARACTER_ROLE_RULES for hint in hints):
            score += 12
        candidates.append((score, text))
    if not candidates:
        for message in messages:
            text = clean_text(message.text)
            if text:
                candidates.append((len(text), text))
    if not candidates:
        return "今天有持续参与，但没有适合公开摘录的长句。"
    candidates.sort(reverse=True)
    best = candidates[0][1]
    return best[:58] + ("..." if len(best) > 58 else "")


def _character_story_function(role_label: str, message_count: int, topic_count: int) -> str:
    role_functions = {
        "活动推进者": "推进剧情",
        "资料投喂员": "递道具",
        "问题发射台": "抛冲突",
        "现场解法师": "给解法",
        "气氛承包人": "接气口",
        "在场感选手": "稳住场",
    }
    function = role_functions.get(role_label, "补场面")
    if message_count >= 8:
        return f"{function} · 高频出场"
    if topic_count >= 4:
        return f"{function} · 多线串联"
    return function


def _character_callback_hint(role_label: str, evidence: str, memory_label: str) -> str:
    if memory_label:
        return f"今天不是孤例，可以回看「{role_label}」的长期复现"
    if evidence:
        return f"如果后续继续出现，可沉淀为「{role_label}」回调"
    return f"今日暂记为「{role_label}」出场"


def _character_arc_label(role_label: str, memory: CharacterMemory | None, message_count: int) -> str:
    if memory and memory.recent_fact_count >= 4:
        recurrence_label = memory.recurrence_label or memory_recurrence_label(memory.recent_fact_count)
        return f"{recurrence_label}，今天继续以「{role_label}」推进"
    if memory and memory.lifetime_fact_count > 0:
        depth_label = memory.depth_label or memory_depth_label(memory.lifetime_fact_count)
        return f"{depth_label}，今日再次露出「{role_label}」信号"
    if message_count >= 5:
        return f"今日高频出场，先形成「{role_label}」日线"
    return f"今日新鲜出场，暂记「{role_label}」"


def _character_meme_seed(
    role_label: str,
    topic_count: int,
    evidence: str,
    memory: CharacterMemory | None,
) -> str:
    if memory:
        return memory.callback_seed or memory_callback_seed(role_label, memory.recent_fact_count)
    if topic_count >= 3:
        return f"多话题串场的「{role_label}」"
    token = next((token for token in _tokenize(evidence) if _is_clean_topic(token)), "")
    if token:
        return f"围绕「{token}」的「{role_label}」"
    return f"今日「{role_label}」待观察"


def _profile_evidence_count(
    memory: CharacterMemory | None,
    creative_memory: CreativeProfileMemory | None,
    message_count: int,
    topic_count: int,
    relationship_hint: str,
) -> int:
    count = min(memory.recent_fact_count, 20) if memory else 0
    if creative_memory:
        count = max(count, min(creative_memory.recurrence_evidence_count, 20))
    if message_count >= 2:
        count += 1
    if memory and topic_count >= 2:
        count += 1
    if creative_memory and topic_count >= 1:
        count += 1
    if relationship_hint:
        count += 1
    return count


def profile_upgrade_status(evidence_count: int) -> str:
    return "eligible_for_review" if evidence_count >= 2 else "daily_note_only"


def profile_upgrade_reason(
    evidence_count: int,
    memory: CharacterMemory | None,
    message_count: int,
    topic_count: int,
    relationship_hint: str,
) -> str:
    if evidence_count < 2:
        return "只有单日轻量信号，不能升级为长期人物画像"
    reasons: list[str] = []
    if memory and memory.recent_fact_count > 0:
        reasons.append(f"近{MEMORY_LOOKBACK_DAYS}天已有 {memory.recent_fact_count} 次角色复现")
    if message_count >= 2:
        reasons.append(f"今日同一身份 {message_count} 条发言支撑")
    if topic_count >= 2:
        reasons.append(f"今日跨 {topic_count} 个公开话题出现")
    if relationship_hint:
        reasons.append("今日存在同场关系候选")
    return "；".join(reasons[:3]) or "达到最小复现证据"


def _relation_group_key(message: ReportMessage) -> str:
    if message.person_id:
        return f"person:{message.person_id}"
    name = (message.sender_name or "").strip()
    return f"name:{name}" if name and name != "匿名" else ""


def node_key(label: str) -> str:
    cleaned = re.sub(r"\s+", "-", clean_text(label)).strip("-")
    cleaned = re.sub(r"[^\w\u4e00-\u9fff-]+", "", cleaned)
    return cleaned[:48] or "node"


def character_node_key(group_key: str, name: str) -> str:
    if group_key.startswith("person:"):
        digest = hashlib.sha256(group_key.encode("utf-8")).hexdigest()[:12]
        return f"person-{digest}"
    return node_key(name)


def _relationship_hints(
    messages: list[ReportMessage],
    character_keys: set[str],
    node_key_by_group: dict[str, str],
    name_by_group: dict[str, str],
) -> dict[str, tuple[str, str, str]]:
    topic_groups: dict[str, dict[str, int]] = {}
    for message in messages:
        group_key = _relation_group_key(message)
        if not group_key or group_key not in character_keys:
            continue
        for token in set(_tokenize(message.text)):
            if _is_clean_topic(token):
                topic_groups.setdefault(token, {}).setdefault(group_key, 0)
                topic_groups[token][group_key] += 1

    candidates: dict[str, list[tuple[int, str, str, str]]] = {}
    for topic, counts in topic_groups.items():
        if len(counts) < 2:
            continue
        ranked = sorted(counts.items(), key=lambda item: (-item[1], name_by_group.get(item[0], "")))
        for group_key, count in ranked:
            for peer_key, peer_count in ranked:
                if peer_key == group_key:
                    continue
                peer_name = name_by_group.get(peer_key, "群友")
                peer_node_key = node_key_by_group.get(peer_key, node_key(peer_name))
                score = count + peer_count + len(topic)
                candidates.setdefault(group_key, []).append(
                    (
                        score,
                        f"和{peer_name}围绕「{topic}」同场接力",
                        peer_node_key,
                        topic,
                    )
                )
                break

    hints: dict[str, tuple[str, str, str]] = {}
    for group_key, group_candidates in candidates.items():
        group_candidates.sort(key=lambda item: (-item[0], item[1]))
        _score, label, peer_node_key, topic = group_candidates[0]
        hints[group_key] = (label, peer_node_key, topic)
    return hints


def compute_characters(
    messages: list[ReportMessage],
    memory_by_person: dict[str, CharacterMemory] | None = None,
    creative_memory_by_person: dict[str, CreativeProfileMemory] | None = None,
    limit: int = DEFAULT_CHARACTER_LIMIT,
) -> list[CharacterCard]:
    memory_by_person = memory_by_person or {}
    creative_memory_by_person = creative_memory_by_person or {}
    grouped: dict[str, list[ReportMessage]] = {}
    group_person_ids: dict[str, str] = {}
    for message in messages:
        name = (message.sender_name or "").strip()
        if not name or name == "匿名":
            continue
        if message.person_id:
            group_key = f"person:{message.person_id}"
            group_person_ids[group_key] = message.person_id
        else:
            group_key = f"name:{name}"
        grouped.setdefault(group_key, []).append(message)

    name_by_group: dict[str, str] = {}
    node_key_by_group: dict[str, str] = {}
    for group_key, group in grouped.items():
        names = Counter((message.sender_name or "").strip() for message in group)
        names.pop("", None)
        names.pop("匿名", None)
        name = names.most_common(1)[0][0] if names else "群友"
        name_by_group[group_key] = name
        node_key_by_group[group_key] = character_node_key(group_key, name)
    relationship_hints = _relationship_hints(
        messages,
        set(grouped),
        node_key_by_group,
        name_by_group,
    )

    ranked: list[tuple[float, CharacterCard]] = []
    for group_key, group in grouped.items():
        name = name_by_group.get(group_key, "群友")
        role_label, one_liner, role_score = _character_role(group)
        topic_count = len(
            {
                token
                for message in group
                for token in _tokenize(message.text)
                if _is_clean_topic(token)
            }
        )
        if len(group) < 2 and role_score == 0:
            continue
        word_count = sum(len(clean_text(message.text)) for message in group)
        person_id = group_person_ids.get(group_key)
        memory = memory_by_person.get(person_id) if person_id else None
        creative_memory = creative_memory_by_person.get(person_id) if person_id else None
        memory_score = min(memory.recent_fact_count, 10) if memory else 0
        if creative_memory:
            memory_score += min(creative_memory.recurrence_evidence_count, 8)
        memory_label = ""
        if memory:
            memory_label = (
                f"近{MEMORY_LOOKBACK_DAYS}天 {memory.recent_fact_count} 次角色复现"
                f" · 长期偏「{memory.dominant_role_label}」"
            )
        creative_profile_label = ""
        if creative_memory:
            creative_profile_label = f"已审核创意画像「{creative_memory.role_label}」"
            memory_label = (
                f"{memory_label} · {creative_profile_label}"
                if memory_label
                else creative_profile_label
            )
        evidence = _character_evidence(group)
        relationship_hint, relationship_target_key, relationship_topic = relationship_hints.get(
            group_key,
            ("", "", ""),
        )
        node_key = node_key_by_group.get(group_key, character_node_key(group_key, name))
        profile_evidence_count = _profile_evidence_count(
            memory,
            creative_memory,
            len(group),
            topic_count,
            relationship_hint,
        )
        upgrade_reason_text = profile_upgrade_reason(
            profile_evidence_count,
            memory,
            len(group),
            topic_count,
            relationship_hint,
        )
        if creative_memory:
            upgrade_reason_text = (
                f"已审核 creative_profile 复用；{upgrade_reason_text}"
                if upgrade_reason_text
                else "已审核 creative_profile 复用"
            )
        memory_weight_label_text = "只按今日表现呈现"
        if memory:
            memory_weight_label_text = memory.memory_weight_label or memory_weight_label(
                memory.recent_fact_count,
                memory.lifetime_fact_count,
            )
        if creative_memory and creative_memory.memory_weight_label:
            memory_weight_label_text = creative_memory.memory_weight_label
        score = (
            len(group) * 3
            + role_score * 4
            + min(topic_count, 6)
            + min(word_count / 80, 4)
            + memory_score
        )
        ranked.append(
            (
                score,
                CharacterCard(
                    rank=0,
                    name=name,
                    role_label=role_label,
                    one_liner=one_liner,
                    evidence=evidence,
                    message_count=len(group),
                    topic_count=topic_count,
                    node_key=node_key,
                    memory_label=memory_label,
                    member_fact_memory_used=memory is not None,
                    story_function=creative_memory.story_function
                    if creative_memory and creative_memory.story_function
                    else _character_story_function(role_label, len(group), topic_count),
                    callback_hint=creative_memory.callback_hint
                    if creative_memory and creative_memory.callback_hint
                    else _character_callback_hint(role_label, evidence, memory_label),
                    arc_label=creative_memory.daily_arc
                    if creative_memory and creative_memory.daily_arc
                    else _character_arc_label(role_label, memory, len(group)),
                    relationship_hint=relationship_hint,
                    relationship_target_key=relationship_target_key,
                    relationship_topic=relationship_topic,
                    meme_seed=creative_memory.meme_seed
                    if creative_memory and creative_memory.meme_seed
                    else _character_meme_seed(role_label, topic_count, evidence, memory),
                    memory_weight_label=memory_weight_label_text,
                    evidence_anchor=f"daily_character_note:{node_key}",
                    expressive_label=creative_memory.expressive_label
                    if creative_memory and creative_memory.expressive_label
                    else "",
                    profile_evidence_count=profile_evidence_count,
                    profile_upgrade_status=profile_upgrade_status(profile_evidence_count),
                    profile_upgrade_reason=upgrade_reason_text,
                    creative_profile_label=creative_profile_label,
                    creative_profile_status="active_reviewed" if creative_memory else "",
                ),
            )
        )

    ranked.sort(key=lambda item: (-item[0], item[1].name))
    characters = [card for _score, card in ranked[:limit]]
    for index, character in enumerate(characters, start=1):
        character.rank = index
        color_bg, color_text = CASE_CARD_COLORS[(index - 1) % len(CASE_CARD_COLORS)]
        character.color_bg = color_bg
        character.color_text = color_text
    return characters


# ---------------------------------------------------------------------------
# Highlight / timeline
# ---------------------------------------------------------------------------

def extract_highlight(messages: list[ReportMessage]) -> str | None:
    """Pick one real, quotable group message for the '今日高亮' block."""
    candidates = []
    for m in messages:
        text = clean_text(m.text)
        if len(text) < 20:
            continue
        if "接龙" in text or text.startswith("打卡") or _looks_promotional_noise(text):
            continue
        score = min(len(text), 120)
        if any(word in text for word in HIGHLIGHT_SIGNAL_WORDS):
            score += 35
        if len(text) > 180:
            score -= 25
        candidates.append((score, len(text), text))
    if not candidates:
        return None
    candidates.sort(reverse=True)
    best = candidates[0][2]
    return best[:92] + ("…" if len(best) > 92 else "")


def hourly_timeline(messages: list[ReportMessage], start: datetime, buckets: int = DEFAULT_HOURLY_BUCKETS) -> list[int]:
    counts = [0] * buckets
    for msg in messages:
        t = msg.sent_at
        if not t:
            continue
        delta = t - start
        hour = int(delta.total_seconds() // 3600)
        if 0 <= hour < buckets:
            counts[hour] += 1
    return counts
