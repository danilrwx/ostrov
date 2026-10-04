//! CalDAV (any server's: iCloud, Fastmail, Nextcloud...; Google's wants OAuth, not had here): its WebDAV answers
//! read as much as a multistatus needs, the account's calendars found and their events in a span fetched.

use crate::ics::{iso, unfold};

/// How much of a calendar is read at most: an .ics of years of meetings is a few MB.
pub const LIMIT: u64 = 64 << 20;

/// A node of an XML document: its name without its namespace's prefix, its children, its text.
#[derive(Default, Debug)]
struct Node {
    name: String,
    kids: Vec<Node>,
    text: String,
}

impl Node {
    /// The first node of the name under this one, depth first.
    fn find(&self, name: &str) -> Option<&Node> {
        self.kids.iter().find_map(|k| if k.name == name { Some(k) } else { k.find(name) })
    }

    /// Every outermost node of the name under this one.
    fn all(&self, name: &str) -> Vec<&Node> {
        self.kids.iter().flat_map(|k| if k.name == name { vec![k] } else { k.all(name) }).collect()
    }

    /// The text of the first node of the name under this one, "" if none.
    fn text_of(&self, name: &str) -> &str {
        self.find(name).map_or("", |n| n.text.trim())
    }
}

/// XML's entities undone: the five named ones and characters by number.
fn entities(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(j) = rest.find(';') else { break };
        let e = &rest[1..j];
        let c = match e {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => e
                .strip_prefix("#x")
                .map(|h| u32::from_str_radix(h, 16))
                .or_else(|| e.strip_prefix('#').map(str::parse))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[j + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A WebDAV answer as a tree, as much XML as a multistatus is: elements, text, CDATA; the prolog and comments
/// passed over.
fn xml(s: &str) -> Node {
    let mut stack = vec![Node::default()];
    let mut rest = s;
    let close = |stack: &mut Vec<Node>| {
        if stack.len() > 1
            && let Some(n) = stack.pop()
            && let Some(top) = stack.last_mut()
        {
            top.kids.push(n);
        }
    };
    while let Some(i) = rest.find('<') {
        if let Some(top) = stack.last_mut() {
            top.text.push_str(&entities(&rest[..i]));
        }
        rest = &rest[i..];
        if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let end = r.find("]]>").unwrap_or(r.len());
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&r[..end]);
            }
            rest = r.get(end + 3..).unwrap_or("");
            continue;
        }
        let Some(j) = rest.find('>') else { break };
        let tag = &rest[1..j];
        rest = &rest[j + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if tag.starts_with('/') {
            close(&mut stack);
            continue;
        }
        let name = tag.trim_end_matches('/').split_whitespace().next().unwrap_or_default();
        let name = name.rsplit(':').next().unwrap_or_default().to_string();
        stack.push(Node { name, ..Node::default() });
        if tag.ends_with('/') {
            close(&mut stack);
        }
    }
    while stack.len() > 1 {
        close(&mut stack);
    }
    stack.pop().unwrap_or_default()
}

/// The href a PROPFIND's answer gives for prop (current-user-principal, calendar-home-set).
fn href(body: &str, prop: &str) -> Option<String> {
    let h = xml(body).find(prop)?.text_of("href").to_string();
    (!h.is_empty()).then_some(h)
}

/// The calendars of a calendar home's PROPFIND (Depth 1): their hrefs and colours, the collections that are
/// not calendars (the home, the scheduling inbox and outbox) left out.
fn calendars(body: &str) -> Vec<(String, String)> {
    let doc = xml(body);
    doc.all("response")
        .into_iter()
        .filter(|r| r.find("resourcetype").and_then(|t| t.find("calendar")).is_some())
        .filter_map(|r| {
            let href = r.kids.iter().find(|k| k.name == "href")?.text.trim().to_string();
            Some((href, r.text_of("calendar-color").to_string()))
        })
        .collect()
}

/// The iCalendar documents of a calendar-query REPORT's answer.
fn datas(body: &str) -> Vec<String> {
    xml(body).all("calendar-data").into_iter().map(|n| n.text.clone()).collect()
}

/// An href on base's server.
fn join(base: &str, href: &str) -> String {
    if href.starts_with("http") {
        return href.to_string();
    }
    let host_end = base.find("://").map_or(0, |i| i + 3);
    let origin = base[host_end..].find('/').map_or(base, |i| &base[..host_end + i]);
    format!("{origin}{href}")
}

fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    b.chunks(3)
        .flat_map(|c| {
            let n = c.iter().enumerate().fold(0u32, |n, (i, &x)| n | u32::from(x) << (16 - 8 * i));
            (0..4).map(move |i| if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' })
        })
        .collect()
}

/// A WebDAV request (PROPFIND, REPORT) of an XML body, its answer's body; blocking.
fn dav(agent: &ureq::Agent, auth: &str, method: &str, url: &str, depth: &str, body: &str) -> Result<String, String> {
    let req = ureq::http::Request::builder()
        .method(method)
        .uri(url)
        .header("Authorization", auth)
        .header("Depth", depth)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(body.to_string())
        .map_err(|e| e.to_string())?;
    let b = agent.run(req).and_then(|mut r| r.body_mut().with_config().limit(LIMIT).read_to_vec());
    Ok(unfold(&b.map_err(|e| format!("{method} {url}: {e}"))?))
}

/// The iCalendar documents of a CalDAV account's calendars with their colours, of the events between from and
/// to (local seconds): the user's principal, its calendar home, the calendars in it, then each one's events in the
/// span.
pub fn caldav(
    agent: &ureq::Agent,
    root: &str,
    user: &str,
    password: &str,
    from: i64,
    to: i64,
) -> Result<Vec<(String, String)>, String> {
    let auth = format!("Basic {}", base64(format!("{user}:{password}").as_bytes()));
    let dav = |method, url: &str, depth, body: &str| dav(agent, &auth, method, url, depth, body);
    const D: &str = r#"xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav""#;
    let principal = format!("<d:propfind {D}><d:prop><d:current-user-principal/></d:prop></d:propfind>");
    let principal = href(&dav("PROPFIND", root, "0", &principal)?, "current-user-principal")
        .map_or(root.to_string(), |h| join(root, &h));
    let home = format!("<d:propfind {D}><d:prop><c:calendar-home-set/></d:prop></d:propfind>");
    let home = href(&dav("PROPFIND", &principal, "0", &home)?, "calendar-home-set");
    let home = home.map_or(principal, |h| join(root, &h));
    let list = format!(
        "<d:propfind {D} xmlns:a=\"http://apple.com/ns/ical/\"><d:prop><d:resourcetype/><d:displayname/>\
         <a:calendar-color/></d:prop></d:propfind>"
    );
    // the span in UTC, a day wider each side for any zone's offset
    let utc = |t: i64| {
        let s = iso(t).replace(['-', ':'], "");
        format!("{s}Z")
    };
    let query = format!(
        "<c:calendar-query {D}><d:prop><c:calendar-data/></d:prop><c:filter><c:comp-filter name=\"VCALENDAR\">\
         <c:comp-filter name=\"VEVENT\"><c:time-range start=\"{}\" end=\"{}\"/></c:comp-filter></c:comp-filter>\
         </c:filter></c:calendar-query>",
        utc(from - 86400),
        utc(to + 86400)
    );
    let mut out = Vec::new();
    for (href, color) in calendars(&dav("PROPFIND", &home, "1", &list)?) {
        let body = dav("REPORT", &join(root, &href), "1", &query)?;
        out.extend(datas(&body).into_iter().map(|ics| (unfold(ics.as_bytes()), color.clone())));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MULTISTATUS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/">
  <D:response>
    <D:href>/calendars/me%40example.com/</D:href>
    <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype><D:displayname>me</D:displayname>
    </D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
  <D:response>
    <D:href>/calendars/me%40example.com/events-1/</D:href>
    <D:propstat><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
    <D:displayname>Work</D:displayname><A:calendar-color>#49b0ffff</A:calendar-color></D:prop>
    <D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
</D:multistatus>"#;

    const REPORT: &str = "<d:multistatus xmlns:d=\"DAV:\" xmlns:cal=\"urn:ietf:params:xml:ns:caldav\"><d:response>\
<d:href>/a.ics</d:href><d:propstat><d:prop><cal:calendar-data>BEGIN:VCALENDAR&#13;\nBEGIN:VEVENT&#13;\n\
SUMMARY:Q&amp;A&#13;\nEND:VEVENT&#13;\nEND:VCALENDAR&#13;\n</cal:calendar-data></d:prop></d:propstat></d:response>\
<d:response><d:href>/b.ics</d:href><d:propstat><d:prop><cal:calendar-data><![CDATA[BEGIN:VCALENDAR\n\
SUMMARY:<b>\nEND:VCALENDAR]]></cal:calendar-data></d:prop></d:propstat></d:response></d:multistatus>";

    #[test]
    fn caldav_xml() {
        let principal = r#"<multistatus xmlns="DAV:"><response><href>/</href><propstat><prop><current-user-principal>
            <href>/principals/users/me%40example.com/</href></current-user-principal></prop></propstat></response>
            </multistatus>"#;
        assert_eq!(href(principal, "current-user-principal").as_deref(), Some("/principals/users/me%40example.com/"));
        assert_eq!(href(principal, "calendar-home-set"), None);
        assert_eq!(calendars(MULTISTATUS), vec![("/calendars/me%40example.com/events-1/".into(), "#49b0ffff".into())]);
        let d = datas(REPORT);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0], "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Q&A\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n");
        assert!(d[1].contains("SUMMARY:<b>"));
        assert_eq!(join("https://caldav.example.com", "/calendars/x/"), "https://caldav.example.com/calendars/x/");
        assert_eq!(join("https://caldav.example.com/", "/c/"), "https://caldav.example.com/c/");
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64(b"ab"), "YWI=");
    }
}
