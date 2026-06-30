#!/usr/bin/env python3
"""Generate public-medium email fixtures for the email extractor regression gate.

Produces:
  - testdata/fixtures/public-medium/email/medium-eml/*.eml (13 samples)
  - testdata/fixtures/public-medium/email/medium-mbox/thunderbird_takeout.mbox (55 messages)
  - README.md and expected.json for each subdirectory

All data is synthetic; no personal information is used.
"""

import json
import random
from datetime import datetime, timedelta, timezone
from email import message_from_bytes
from email.header import Header, decode_header, make_header
from email.mime.application import MIMEApplication
from email.utils import formataddr
from email.mime.image import MIMEImage
from email.mime.message import MIMEMessage
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText
from email.utils import format_datetime, make_msgid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURE_DIR = ROOT / "testdata" / "fixtures" / "public-medium" / "email"
EML_DIR = FIXTURE_DIR / "medium-eml"
MBOX_DIR = FIXTURE_DIR / "medium-mbox"

random.seed(42)


def rfc2822_date(dt: datetime) -> str:
    return format_datetime(dt, usegmt=True)


def build_plain_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEText(f"This is plain text message number {index}.\nNo attachments.\n", "plain", "utf-8")
    msg["From"] = "alice@example.com"
    msg["To"] = "bob@example.com"
    msg["Subject"] = f"Plain text message {index}"
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    msg["X-Mailer"] = "Python unittest generator"
    return msg.as_bytes()


def build_html_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEMultipart("alternative")
    msg["From"] = "Alice <alice@example.com>"
    msg["To"] = "Bob <bob@example.com>, Carol <carol@example.com>"
    msg["Cc"] = "Dave <dave@example.com>"
    msg["Subject"] = f"HTML message {index}"
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    msg.attach(MIMEText(f"Plain fallback for message {index}.", "plain", "utf-8"))
    msg.attach(MIMEText(
        f"<html><body><p>HTML body for message <b>{index}</b>.</p></body></html>",
        "html", "utf-8",
    ))
    return msg.as_bytes()


def build_attachment_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEMultipart("mixed")
    msg["From"] = "sender@example.com"
    msg["To"] = "recipient@example.com"
    msg["Subject"] = f"Document attached {index}"
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    msg.attach(MIMEText(f"Please see the attached document for message {index}.", "plain", "utf-8"))
    payload = f"Attachment payload {index}".encode()
    att = MIMEApplication(payload, "octet-stream")
    att.add_header("Content-Disposition", "attachment", filename=f"data{index}.bin")
    msg.attach(att)
    return msg.as_bytes()


def build_multipart_related_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEMultipart("related")
    msg["From"] = "newsletter@example.com"
    msg["To"] = "subscriber@example.com"
    msg["Subject"] = f"Newsletter {index}"
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    msg.attach(MIMEText(f"Newsletter plain text {index}.", "plain", "utf-8"))
    msg.attach(MIMEText(
        f'<html><body><img src="cid:image{index}"><p>Newsletter HTML {index}</p></body></html>',
        "html", "utf-8",
    ))
    image_bytes = bytes([random.randint(0, 255) for _ in range(64)])
    img = MIMEImage(image_bytes, "png")
    img.add_header("Content-ID", f"<image{index}>")
    img.add_header("Content-Disposition", "inline", filename=f"inline{index}.png")
    msg.attach(img)
    return msg.as_bytes()


def build_encoded_headers_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEText(f"中文正文 {index}。\n", "plain", "utf-8")
    msg["From"] = formataddr(("李雷", "lilei@example.com"))
    msg["To"] = formataddr(("韩梅梅", "hanmeimei@example.com"))
    msg["Subject"] = Header(f"中文主题 {index}", "utf-8")
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    return msg.as_bytes()


def build_japanese_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEText(f"これは日本語のメッセージ {index} です。\n", "plain", "utf-8")
    msg["From"] = formataddr(("山田太郎", "yamada@example.jp"))
    msg["To"] = "tanaka@example.jp"
    msg["Subject"] = Header(f"日本語の件名 {index}", "utf-8")
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.jp")
    return msg.as_bytes()


def build_thread_eml(index: int, dt: datetime, parent_id: str | None) -> bytes:
    msg = MIMEText(f"Reply {index} in the thread.\n", "plain", "utf-8")
    msg["From"] = f"user{index}@example.com"
    msg["To"] = "thread@example.com"
    msg["Subject"] = "Re: project plan"
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    if parent_id:
        msg["In-Reply-To"] = parent_id
        msg["References"] = parent_id
    msg["Received"] = f"from mail.example.com by mx.example.com with ESMTPS id abc{index}; {rfc2822_date(dt - timedelta(seconds=5))}"
    return msg.as_bytes()


def build_x_headers_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEText(f"Message with X-Headers {index}.\n", "plain", "utf-8")
    msg["From"] = "remote@example.com"
    msg["To"] = "local@example.com"
    msg["Subject"] = f"X-Headers {index}"
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    msg["X-Mailer"] = "CustomMailer/2.0"
    msg["X-Originating-IP"] = f"192.168.1.{index % 256}"
    return msg.as_bytes()


def build_qp_body_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEText(
        f"Quoted printable body with special chars: café, naïve, message {index}.\n",
        "plain", "utf-8",
    )
    msg["From"] = "qp@example.com"
    msg["To"] = "dest@example.com"
    msg["Subject"] = f"Quoted-printable {index}"
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    del msg["Content-Transfer-Encoding"]
    msg["Content-Transfer-Encoding"] = "quoted-printable"
    return msg.as_bytes()


def build_no_subject_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEText(f"This message has no subject, number {index}.\n", "plain", "utf-8")
    msg["From"] = "nosubject@example.com"
    msg["To"] = "recipient@example.com"
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    return msg.as_bytes()


def build_long_subject_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEText(f"Message with long subject {index}.\n", "plain", "utf-8")
    msg["From"] = "longsubject@example.com"
    msg["To"] = "recipient@example.com"
    msg["Subject"] = f"Very long subject line repeated for message {index}: " + "word " * 20
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    return msg.as_bytes()


def build_nested_rfc822_eml(index: int, dt: datetime) -> bytes:
    msg = MIMEMultipart("mixed")
    msg["From"] = "forwarder@example.com"
    msg["To"] = "archive@example.com"
    msg["Subject"] = f"Forwarded message {index}"
    msg["Date"] = rfc2822_date(dt)
    msg["Message-ID"] = make_msgid(domain="example.com")
    msg.attach(MIMEText(f"See attached forwarded message {index}.\n", "plain", "utf-8"))
    inner = MIMEText(f"Original body {index}.\n", "plain", "utf-8")
    inner["From"] = "original@example.com"
    inner["To"] = "final@example.com"
    inner["Subject"] = f"Original message {index}"
    inner["Date"] = rfc2822_date(dt - timedelta(hours=1))
    inner["Message-ID"] = make_msgid(domain="example.com")
    msg.attach(MIMEMessage(inner))
    return msg.as_bytes()


EML_BUILDERS = [
    ("plain", build_plain_eml),
    ("html", build_html_eml),
    ("attachment", build_attachment_eml),
    ("multipart_related", build_multipart_related_eml),
    ("encoded_headers", build_encoded_headers_eml),
    ("japanese", build_japanese_eml),
    ("thread_root", lambda i, dt: build_thread_eml(i, dt, None)),
    ("thread_reply", lambda i, dt: build_thread_eml(i, dt, "<parent-message-id@example.com>")),
    ("x_headers", build_x_headers_eml),
    ("quoted_printable", build_qp_body_eml),
    ("no_subject", build_no_subject_eml),
    ("long_subject", build_long_subject_eml),
    ("nested_rfc822", build_nested_rfc822_eml),
]


def _decode_header(value):
    if value is None:
        return ""
    return str(make_header(decode_header(value)))


def extract_summary(data: bytes) -> dict:
    parsed = message_from_bytes(data)
    subject = _decode_header(parsed["Subject"])
    from_addr = _decode_header(parsed["From"])
    if "<" in from_addr:
        from_email = from_addr.split("<")[-1].rstrip(">").strip()
    else:
        parts = from_addr.split()
        from_email = parts[0] if parts else ""
    return {"subjectContains": subject[:40], "fromContains": from_email}


def generate_eml_fixtures():
    EML_DIR.mkdir(parents=True, exist_ok=True)
    expected = []
    base_dt = datetime(2024, 3, 1, 9, 0, 0, tzinfo=timezone.utc)

    for idx, (name, builder) in enumerate(EML_BUILDERS):
        dt = base_dt + timedelta(hours=idx)
        data = builder(idx, dt)
        filename = f"{name}.eml"
        path = EML_DIR / filename
        path.write_bytes(data)
        expected.append({
            "file": filename,
            "type": "eml",
            "expected": extract_summary(data),
        })

    readme = EML_DIR / "README.md"
    readme.write_text(
        "# public-medium EML fixtures\n\n"
        "Synthetic EML samples covering common forensic email scenarios.\n\n"
        "## Samples\n\n"
        "| File | Purpose |\n"
        "|------|---------|\n"
        "| plain.eml | Single-part plain text |\n"
        "| html.eml | multipart/alternative plain+HTML, Cc |\n"
        "| attachment.eml | multipart/mixed with base64 attachment |\n"
        "| multipart_related.eml | multipart/related with inline image |\n"
        "| encoded_headers.eml | RFC 2047 encoded Chinese headers |\n"
        "| japanese.eml | UTF-8 Japanese headers and body |\n"
        "| thread_root.eml | Message-ID for threading |\n"
        "| thread_reply.eml | In-Reply-To / References |\n"
        "| x_headers.eml | X-Mailer and X-Originating-IP |\n"
        "| quoted_printable.eml | Quoted-printable transfer encoding |\n"
        "| no_subject.eml | Missing Subject header |\n"
        "| long_subject.eml | Long folded Subject header |\n"
        "| nested_rfc822.eml | message/rfc822 forward attachment |\n\n"
        "## Visibility\n\npublic-medium\n\n"
        "## Source\n\nSynthetic samples. No personal data.\n\n"
        "## Expected JSON\n\n`expected.json` in this directory.\n",
        encoding="utf-8",
    )

    expected_path = EML_DIR / "expected.json"
    expected_path.write_text(json.dumps({"samples": expected}, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"Generated {len(EML_BUILDERS)} EML fixtures in {EML_DIR}")


def generate_mbox_fixture():
    MBOX_DIR.mkdir(parents=True, exist_ok=True)
    filename = "thunderbird_takeout.mbox"
    path = MBOX_DIR / filename
    base_dt = datetime(2024, 4, 1, 8, 0, 0, tzinfo=timezone.utc)
    lines = []

    senders = [
        ("alice@example.com", "Alice"),
        ("bob@example.com", "Bob"),
        ("carol@example.com", "Carol"),
        ("dave@example.com", "Dave"),
        ("newsletter@example.com", "Newsletter"),
    ]
    subjects = [
        "Project update",
        "Lunch tomorrow?",
        "Invoice attached",
        "Meeting notes",
        "Re: proposal",
        "Weekly report",
        "FW: announcement",
        "Account alert",
        "Holiday schedule",
        "System notification",
    ]

    for i in range(55):
        sender_email, sender_name = senders[i % len(senders)]
        subject = subjects[i % len(subjects)]
        dt = base_dt + timedelta(minutes=i * 13)
        msg_id = make_msgid(domain="example.com")
        body = f"Body of message {i+1} from {sender_name}.\n"
        if i % 7 == 0:
            body = body.replace(".", ".\n>From escaped line should be unescaped.", 1)
        lines.append(f"From {sender_email} {dt.strftime('%a %b %d %H:%M:%S %Y')}\n")
        lines.append(f"From: {sender_name} <{sender_email}>\n")
        lines.append("To: team@example.com\n")
        lines.append(f"Subject: {subject} #{i+1}\n")
        lines.append(f"Date: {rfc2822_date(dt)}\n")
        lines.append(f"Message-ID: {msg_id}\n")
        lines.append("Content-Type: text/plain; charset=utf-8\n")
        lines.append("\n")
        lines.append(body)
        lines.append("\n")

    path.write_text("".join(lines), encoding="utf-8")

    readme = MBOX_DIR / "README.md"
    readme.write_text(
        "# public-medium MBOX fixture\n\n"
        "Synthetic Thunderbird-style mbox with 55 messages.\n\n"
        "## Samples\n\n"
        "| File | Type | Purpose |\n"
        "|------|------|---------|\n"
        f"| `{filename}` | mboxrd-style | 55 mixed messages, 5 rotating senders, mboxrd `>From ` escaping |\n\n"
        "## Visibility\n\npublic-medium\n\n"
        "## Source\n\nSynthetic. No personal data.\n\n"
        "## Expected JSON\n\n`expected.json` in this directory.\n",
        encoding="utf-8",
    )

    expected_path = MBOX_DIR / "expected.json"
    expected_path.write_text(
        json.dumps({
            "samples": [{
                "file": filename,
                "type": "mbox",
                "expected": {
                    "messagesCount": 55,
                    "firstMessage": {
                        "fromContains": "alice@example.com",
                        "subjectContains": "Project update #1",
                        "bodyContains": "Body of message 1",
                    },
                    "lastMessage": {
                        "fromContains": "newsletter@example.com",
                        "subjectContains": "System notification #55",
                        "bodyContains": "Body of message 55",
                    },
                }
            }]
        }, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    print(f"Generated {filename} with 55 messages in {MBOX_DIR}")


def main():
    generate_eml_fixtures()
    generate_mbox_fixture()
    print("Done.")


if __name__ == "__main__":
    main()
