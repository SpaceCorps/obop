//! The forum's data and rules: categories, threads, posts, members, the moderation audit log.
//! Everything is held in memory behind one lock and written through to the storage backend as
//! JSON documents (like `obop-platform`), so any of the three backends works. One process owns
//! the forum; running two against the same database is not supported.
//!
//! All time-dependent rules take `now` explicitly so they can be tested.

use crate::md;
use crate::util;
use obop_storage::{Backend, Fields};
use serde_json::{Value as J, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

pub const POSTS_PER_HOUR: usize = 10;
pub const THREADS_PER_HOUR: usize = 3;
pub const EDIT_WINDOW_SECS: u64 = 15 * 60;
pub const NEW_ACCOUNT_SECS: u64 = 24 * 3600;
pub const MAX_LINKS_NEW_ACCOUNT: usize = 2;
pub const MAX_TITLE: usize = 140;
pub const MAX_BODY: usize = 20_000;
pub const PAGE_THREADS: usize = 25;
pub const PAGE_POSTS: usize = 30;
pub const PAGE_SEARCH: usize = 20;

const T_CAT: &str = "community_category";
const T_THREAD: &str = "community_thread";
const T_POST: &str = "community_post";
const T_MEMBER: &str = "community_member";
const T_AUDIT: &str = "community_audit";

/// Slug, order, staff-only. Names and descriptions come from the language catalogue.
const SEED: [(&str, bool); 8] = [
    ("announcements", true),
    ("general", false),
    ("help", false),
    ("show-and-tell", false),
    ("modules", false),
    ("agents-and-mcp", false),
    ("self-hosting", false),
    ("feature-requests", false),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub slug: String,
    pub name: String,
    pub order: u32,
    pub staff_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: u64,
    pub title: String,
    pub cat: String,
    pub author: String,
    pub created: u64,
    pub last_activity: u64,
    /// Last change of any kind, for `Last-Modified`.
    pub updated: u64,
    pub pinned: bool,
    pub locked: bool,
    pub hidden: bool,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: u64,
    pub thread: u64,
    pub author: String,
    pub body: String,
    pub created: u64,
    pub edited: Option<u64>,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub email: String,
    pub name: String,
    pub joined: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audit {
    pub id: u64,
    pub at: u64,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub detail: String,
}

/// What can go wrong; `Invalid` and `Rate` carry a catalogue key for the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FErr {
    NotFound,
    Gone,
    Forbidden,
    Rate(&'static str),
    Invalid(&'static str),
    Storage(String),
}

impl From<String> for FErr {
    fn from(e: String) -> FErr {
        FErr::Storage(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModAction {
    Pin,
    Unpin,
    Lock,
    Unlock,
    Hide,
    Unhide,
    Delete,
    Move(String),
}

impl ModAction {
    pub fn parse(action: &str, arg: &str) -> Option<ModAction> {
        Some(match action {
            "pin" => ModAction::Pin,
            "unpin" => ModAction::Unpin,
            "lock" => ModAction::Lock,
            "unlock" => ModAction::Unlock,
            "hide" => ModAction::Hide,
            "unhide" => ModAction::Unhide,
            "delete" => ModAction::Delete,
            "move" => ModAction::Move(arg.to_string()),
            _ => return None,
        })
    }

    fn name(&self) -> &'static str {
        match self {
            ModAction::Pin => "pin",
            ModAction::Unpin => "unpin",
            ModAction::Lock => "lock",
            ModAction::Unlock => "unlock",
            ModAction::Hide => "hide",
            ModAction::Unhide => "unhide",
            ModAction::Delete => "delete",
            ModAction::Move(_) => "move",
        }
    }
}

#[derive(Default)]
pub struct Data {
    pub categories: Vec<Category>,
    pub threads: BTreeMap<u64, Thread>,
    pub posts: BTreeMap<u64, Post>,
    pub by_thread: HashMap<u64, Vec<u64>>,
    pub members: HashMap<String, Member>,
    names: HashMap<String, String>,
    pub audit: Vec<Audit>,
    next_thread: u64,
    next_post: u64,
    next_audit: u64,
}

pub struct Forum {
    backend: Arc<dyn Backend>,
    data: Mutex<Data>,
    admins: HashSet<String>,
}

fn s(f: &Fields, k: &str) -> String {
    f.get(k).and_then(J::as_str).unwrap_or("").to_string()
}
fn n(f: &Fields, k: &str) -> u64 {
    f.get(k).and_then(J::as_u64).unwrap_or(0)
}
fn b(f: &Fields, k: &str) -> bool {
    f.get(k).and_then(J::as_bool).unwrap_or(false)
}

fn fields(v: J) -> Fields {
    match v {
        J::Object(m) => m,
        _ => Fields::new(),
    }
}

/// Insert or update a document.
pub(crate) fn put(backend: &dyn Backend, table: &str, id: &str, f: Fields) -> Result<(), String> {
    if backend.update(table, id, &f)? {
        Ok(())
    } else {
        backend.insert(table, id, &f)
    }
}

const RESERVED: [&str; 4] = ["obop", "admin", "staff", "moderator"];

/// Display names: 3 to 24 of `A-Za-z0-9_-`.
pub fn valid_name_shape(name: &str) -> bool {
    (3..=24).contains(&name.chars().count())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Names that imitate the project or its staff.
pub fn is_reserved_name(name: &str) -> bool {
    let norm: String = name
        .to_ascii_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    RESERVED.iter().any(|r| norm.contains(r))
}

pub fn valid_email(e: &str) -> bool {
    let Some((local, domain)) = e.split_once('@') else {
        return false;
    };
    e.len() <= 120
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !e.chars().any(|c| c.is_whitespace() || c.is_control())
        && e.matches('@').count() == 1
}

/// A display-name candidate from the part of the email before the `@`.
pub fn derive_name(email: &str) -> String {
    let local = email.split('@').next().unwrap_or("");
    let mut out = String::new();
    for c in local.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let mut out: String = out.trim_matches('-').chars().take(24).collect();
    while out.chars().count() < 3 {
        out.push('x');
    }
    out
}

impl Forum {
    /// Load the forum from `backend`, seeding the categories on first run.
    pub fn open(backend: Arc<dyn Backend>, admins: HashSet<String>) -> Result<Forum, String> {
        let mut d = Data::default();
        if backend.count(T_CAT)? == 0 {
            for (i, (slug, staff_only)) in SEED.iter().enumerate() {
                put(
                    &*backend,
                    T_CAT,
                    slug,
                    fields(json!({"name": slug, "order": i, "staff_only": staff_only})),
                )?;
            }
        }
        for (id, f) in backend.list(T_CAT)? {
            d.categories.push(Category {
                slug: id,
                name: s(&f, "name"),
                order: n(&f, "order") as u32,
                staff_only: b(&f, "staff_only"),
            });
        }
        d.categories.sort_by_key(|c| c.order);
        for (id, f) in backend.list(T_THREAD)? {
            let Ok(idn) = id.parse::<u64>() else { continue };
            d.next_thread = d.next_thread.max(idn);
            d.threads.insert(
                idn,
                Thread {
                    id: idn,
                    title: s(&f, "title"),
                    cat: s(&f, "cat"),
                    author: s(&f, "author"),
                    created: n(&f, "created"),
                    last_activity: n(&f, "last_activity"),
                    updated: n(&f, "updated"),
                    pinned: b(&f, "pinned"),
                    locked: b(&f, "locked"),
                    hidden: b(&f, "hidden"),
                    deleted: b(&f, "deleted"),
                },
            );
        }
        for (id, f) in backend.list(T_POST)? {
            let Ok(idn) = id.parse::<u64>() else { continue };
            d.next_post = d.next_post.max(idn);
            let p = Post {
                id: idn,
                thread: n(&f, "thread"),
                author: s(&f, "author"),
                body: s(&f, "body"),
                created: n(&f, "created"),
                edited: f.get("edited").and_then(J::as_u64),
                hidden: b(&f, "hidden"),
            };
            d.by_thread.entry(p.thread).or_default().push(idn);
            d.posts.insert(idn, p);
        }
        for v in d.by_thread.values_mut() {
            v.sort_unstable();
        }
        for (id, f) in backend.list(T_MEMBER)? {
            let m = Member {
                email: id.clone(),
                name: s(&f, "name"),
                joined: n(&f, "joined"),
            };
            d.names.insert(m.name.to_ascii_lowercase(), id.clone());
            d.members.insert(id, m);
        }
        for (id, f) in backend.list(T_AUDIT)? {
            let Ok(idn) = id.parse::<u64>() else { continue };
            d.next_audit = d.next_audit.max(idn);
            d.audit.push(Audit {
                id: idn,
                at: n(&f, "at"),
                actor: s(&f, "actor"),
                action: s(&f, "action"),
                target: s(&f, "target"),
                detail: s(&f, "detail"),
            });
        }
        d.audit.sort_by_key(|a| a.id);
        Ok(Forum {
            backend,
            data: Mutex::new(d),
            admins,
        })
    }

    pub fn backend(&self) -> &Arc<dyn Backend> {
        &self.backend
    }

    pub fn is_staff(&self, email: &str) -> bool {
        self.admins.contains(&email.trim().to_ascii_lowercase())
    }

    pub fn lock(&self) -> MutexGuard<'_, Data> {
        // A poisoned lock means a handler panicked mid-update; the data is still structurally
        // valid (every mutation is a single insert), so keep serving.
        self.data.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---------- members ----------

    /// Create the forum profile for `email`. `display` is the chosen name, or derived.
    pub fn register(&self, email: &str, display: Option<&str>, now: u64) -> Result<Member, FErr> {
        let email = email.trim().to_ascii_lowercase();
        let staff = self.is_staff(&email);
        let mut d = self.lock();
        if let Some(m) = d.members.get(&email) {
            return Ok(m.clone());
        }
        let name = match display.map(str::trim).filter(|x| !x.is_empty()) {
            Some(chosen) => {
                if !valid_name_shape(chosen) || (!staff && is_reserved_name(chosen)) {
                    return Err(FErr::Invalid("err.name_invalid"));
                }
                if d.names.contains_key(&chosen.to_ascii_lowercase()) {
                    return Err(FErr::Invalid("err.name_taken"));
                }
                chosen.to_string()
            }
            None => {
                let mut base = derive_name(&email);
                if !staff && is_reserved_name(&base) {
                    base = "member".into();
                }
                let mut candidate = base.clone();
                let mut i = 1;
                while d.names.contains_key(&candidate.to_ascii_lowercase()) {
                    i += 1;
                    let suffix = format!("{i}");
                    let keep: String = base.chars().take(24 - suffix.len()).collect();
                    candidate = format!("{keep}{suffix}");
                }
                candidate
            }
        };
        let m = Member {
            email: email.clone(),
            name: name.clone(),
            joined: now,
        };
        put(
            &*self.backend,
            T_MEMBER,
            &email,
            fields(json!({"name": name, "joined": now})),
        )?;
        d.names.insert(name.to_ascii_lowercase(), email.clone());
        d.members.insert(email, m.clone());
        Ok(m)
    }

    pub fn name_available(&self, name: &str) -> bool {
        !self.lock().names.contains_key(&name.to_ascii_lowercase())
    }

    // ---------- writes ----------

    fn check_title(title: &str) -> Result<String, FErr> {
        let t = util::clean_line(title);
        if t.is_empty() {
            return Err(FErr::Invalid("err.title_required"));
        }
        if t.chars().count() > MAX_TITLE {
            return Err(FErr::Invalid("err.title_long"));
        }
        Ok(t)
    }

    fn check_body(&self, d: &Data, email: &str, body: &str, now: u64) -> Result<String, FErr> {
        let body = body.replace("\r\n", "\n");
        let body = body.trim().to_string();
        if body.is_empty() {
            return Err(FErr::Invalid("err.body_required"));
        }
        if body.chars().count() > MAX_BODY {
            return Err(FErr::Invalid("err.body_long"));
        }
        let brand_new = d
            .members
            .get(email)
            .is_none_or(|m| now.saturating_sub(m.joined) < NEW_ACCOUNT_SECS);
        if brand_new && !self.is_staff(email) && md::link_count(&body) > MAX_LINKS_NEW_ACCOUNT {
            return Err(FErr::Invalid("err.links"));
        }
        Ok(body)
    }

    fn rate_check(&self, d: &Data, email: &str, now: u64, thread: bool) -> Result<(), FErr> {
        if self.is_staff(email) {
            return Ok(());
        }
        let since = now.saturating_sub(3600);
        if thread {
            let c = d
                .threads
                .values()
                .filter(|t| t.author == email && t.created > since)
                .count();
            if c >= THREADS_PER_HOUR {
                return Err(FErr::Rate("err.rate_threads"));
            }
        }
        let c = d
            .posts
            .values()
            .filter(|p| p.author == email && p.created > since)
            .count();
        if c >= POSTS_PER_HOUR {
            return Err(FErr::Rate("err.rate_posts"));
        }
        Ok(())
    }

    fn save_thread(&self, t: &Thread) -> Result<(), FErr> {
        put(
            &*self.backend,
            T_THREAD,
            &t.id.to_string(),
            fields(json!({
                "title": t.title, "cat": t.cat, "author": t.author, "created": t.created,
                "last_activity": t.last_activity, "updated": t.updated, "pinned": t.pinned,
                "locked": t.locked, "hidden": t.hidden, "deleted": t.deleted,
            })),
        )
        .map_err(FErr::Storage)
    }

    fn save_post(&self, p: &Post) -> Result<(), FErr> {
        put(
            &*self.backend,
            T_POST,
            &p.id.to_string(),
            fields(json!({
                "thread": p.thread, "author": p.author, "body": p.body, "created": p.created,
                "edited": p.edited, "hidden": p.hidden,
            })),
        )
        .map_err(FErr::Storage)
    }

    pub fn create_thread(
        &self,
        email: &str,
        cat: &str,
        title: &str,
        body: &str,
        now: u64,
    ) -> Result<u64, FErr> {
        let mut d = self.lock();
        if !d.members.contains_key(email) {
            return Err(FErr::Forbidden);
        }
        let category = d
            .categories
            .iter()
            .find(|c| c.slug == cat)
            .ok_or(FErr::Invalid("err.category_unknown"))?;
        if category.staff_only && !self.is_staff(email) {
            return Err(FErr::Forbidden);
        }
        let title = Self::check_title(title)?;
        let body = self.check_body(&d, email, body, now)?;
        self.rate_check(&d, email, now, true)?;
        d.next_thread += 1;
        d.next_post += 1;
        let t = Thread {
            id: d.next_thread,
            title,
            cat: cat.to_string(),
            author: email.to_string(),
            created: now,
            last_activity: now,
            updated: now,
            pinned: false,
            locked: false,
            hidden: false,
            deleted: false,
        };
        let p = Post {
            id: d.next_post,
            thread: t.id,
            author: email.to_string(),
            body,
            created: now,
            edited: None,
            hidden: false,
        };
        self.save_thread(&t)?;
        self.save_post(&p)?;
        let id = t.id;
        d.by_thread.entry(id).or_default().push(p.id);
        d.posts.insert(p.id, p);
        d.threads.insert(id, t);
        Ok(id)
    }

    pub fn reply(&self, email: &str, thread: u64, body: &str, now: u64) -> Result<u64, FErr> {
        let staff = self.is_staff(email);
        let mut d = self.lock();
        if !d.members.contains_key(email) {
            return Err(FErr::Forbidden);
        }
        let t = d.threads.get(&thread).ok_or(FErr::NotFound)?;
        if t.deleted {
            return Err(FErr::Gone);
        }
        if (t.locked || t.hidden) && !staff {
            return Err(FErr::Forbidden);
        }
        let body = self.check_body(&d, email, body, now)?;
        self.rate_check(&d, email, now, false)?;
        d.next_post += 1;
        let p = Post {
            id: d.next_post,
            thread,
            author: email.to_string(),
            body,
            created: now,
            edited: None,
            hidden: false,
        };
        self.save_post(&p)?;
        let pid = p.id;
        d.by_thread.entry(thread).or_default().push(pid);
        d.posts.insert(pid, p);
        if let Some(t) = d.threads.get_mut(&thread) {
            t.last_activity = now;
            t.updated = now;
            let t = t.clone();
            self.save_thread(&t)?;
        }
        Ok(pid)
    }

    /// The author may edit their own visible post for 15 minutes.
    pub fn edit_post(&self, email: &str, post: u64, body: &str, now: u64) -> Result<u64, FErr> {
        let mut d = self.lock();
        let p = d.posts.get(&post).ok_or(FErr::NotFound)?;
        let t = d.threads.get(&p.thread).ok_or(FErr::NotFound)?;
        if t.deleted {
            return Err(FErr::Gone);
        }
        if p.author != email || p.hidden || t.hidden {
            return Err(FErr::Forbidden);
        }
        if now.saturating_sub(p.created) > EDIT_WINDOW_SECS {
            return Err(FErr::Invalid("err.edit_window"));
        }
        let thread = p.thread;
        let body = self.check_body(&d, email, body, now)?;
        if let Some(p) = d.posts.get_mut(&post) {
            p.body = body;
            p.edited = Some(now);
            let p = p.clone();
            self.save_post(&p)?;
        }
        if let Some(t) = d.threads.get_mut(&thread) {
            t.updated = now;
            let t = t.clone();
            self.save_thread(&t)?;
        }
        Ok(thread)
    }

    fn log(
        &self,
        d: &mut Data,
        actor: &str,
        action: &str,
        target: String,
        detail: String,
        now: u64,
    ) -> Result<(), FErr> {
        d.next_audit += 1;
        let a = Audit {
            id: d.next_audit,
            at: now,
            actor: actor.to_string(),
            action: action.to_string(),
            target,
            detail,
        };
        put(
            &*self.backend,
            T_AUDIT,
            &format!("{:010}", a.id),
            fields(
                json!({"at": a.at, "actor": a.actor, "action": a.action, "target": a.target, "detail": a.detail}),
            ),
        )?;
        d.audit.push(a);
        Ok(())
    }

    /// Staff only. Pin, lock, hide, remove or move a thread; each action is audited.
    pub fn moderate_thread(
        &self,
        email: &str,
        thread: u64,
        action: ModAction,
        now: u64,
    ) -> Result<(), FErr> {
        if !self.is_staff(email) {
            return Err(FErr::Forbidden);
        }
        let mut d = self.lock();
        let actor = d
            .members
            .get(email)
            .map_or_else(|| "staff".to_string(), |m| m.name.clone());
        let t = d.threads.get(&thread).ok_or(FErr::NotFound)?.clone();
        if t.deleted {
            return Err(FErr::Gone);
        }
        let mut t2 = t.clone();
        let mut detail = String::new();
        match &action {
            ModAction::Pin => t2.pinned = true,
            ModAction::Unpin => t2.pinned = false,
            ModAction::Lock => t2.locked = true,
            ModAction::Unlock => t2.locked = false,
            ModAction::Hide => t2.hidden = true,
            ModAction::Unhide => t2.hidden = false,
            ModAction::Delete => {
                t2.deleted = true;
                t2.title.clear();
                // Erase the text, not only the flag.
                let ids = d.by_thread.get(&thread).cloned().unwrap_or_default();
                for pid in ids {
                    if let Some(p) = d.posts.get_mut(&pid) {
                        p.body.clear();
                        let p = p.clone();
                        self.save_post(&p)?;
                    }
                }
            }
            ModAction::Move(slug) => {
                if !d.categories.iter().any(|c| &c.slug == slug) {
                    return Err(FErr::Invalid("err.category_unknown"));
                }
                detail = format!("{} -> {}", t.cat, slug);
                t2.cat = slug.clone();
            }
        }
        t2.updated = now;
        self.save_thread(&t2)?;
        d.threads.insert(thread, t2);
        self.log(
            &mut d,
            &actor,
            action.name(),
            format!("thread:{thread}"),
            detail,
            now,
        )
    }

    pub fn moderate_post(&self, email: &str, post: u64, hide: bool, now: u64) -> Result<u64, FErr> {
        if !self.is_staff(email) {
            return Err(FErr::Forbidden);
        }
        let mut d = self.lock();
        let actor = d
            .members
            .get(email)
            .map_or_else(|| "staff".to_string(), |m| m.name.clone());
        let p = d.posts.get(&post).ok_or(FErr::NotFound)?.clone();
        let first = d.by_thread.get(&p.thread).and_then(|v| v.first()).copied();
        if first == Some(post) {
            // The opening post is hidden by hiding the thread.
            return Err(FErr::Forbidden);
        }
        if let Some(pm) = d.posts.get_mut(&post) {
            pm.hidden = hide;
            let pm = pm.clone();
            self.save_post(&pm)?;
        }
        if let Some(t) = d.threads.get_mut(&p.thread) {
            t.updated = now;
            let t = t.clone();
            self.save_thread(&t)?;
        }
        self.log(
            &mut d,
            &actor,
            if hide { "hide" } else { "unhide" },
            format!("post:{post}"),
            String::new(),
            now,
        )?;
        Ok(p.thread)
    }
}

impl Data {
    pub fn category(&self, slug: &str) -> Option<&Category> {
        self.categories.iter().find(|c| c.slug == slug)
    }

    pub fn display_name(&self, email: &str) -> String {
        self.members
            .get(email)
            .map_or_else(|| "former member".to_string(), |m| m.name.clone())
    }

    pub fn member_by_name(&self, name: &str) -> Option<&Member> {
        self.names
            .get(&name.to_ascii_lowercase())
            .and_then(|e| self.members.get(e))
    }

    pub fn thread_visible(&self, t: &Thread, staff: bool) -> bool {
        !t.deleted && (!t.hidden || staff)
    }

    pub fn thread_posts(&self, tid: u64) -> Vec<&Post> {
        self.by_thread
            .get(&tid)
            .map(|v| v.iter().filter_map(|id| self.posts.get(id)).collect())
            .unwrap_or_default()
    }

    /// Replies to the opening post that are not hidden.
    pub fn visible_replies(&self, tid: u64) -> usize {
        self.thread_posts(tid)
            .iter()
            .skip(1)
            .filter(|p| !p.hidden)
            .count()
    }

    fn sorted<'a>(&'a self, mut v: Vec<&'a Thread>, pinned_first: bool) -> Vec<&'a Thread> {
        v.sort_by(|a, b| {
            if pinned_first {
                b.pinned.cmp(&a.pinned)
            } else {
                std::cmp::Ordering::Equal
            }
            .then(b.last_activity.cmp(&a.last_activity))
            .then(b.id.cmp(&a.id))
        });
        v
    }

    pub fn category_threads(&self, slug: &str, staff: bool) -> Vec<&Thread> {
        let v = self
            .threads
            .values()
            .filter(|t| t.cat == slug && self.thread_visible(t, staff))
            .collect();
        self.sorted(v, true)
    }

    pub fn latest_threads(&self, limit: usize, staff: bool) -> Vec<&Thread> {
        let v = self
            .threads
            .values()
            .filter(|t| self.thread_visible(t, staff))
            .collect();
        self.sorted(v, false).into_iter().take(limit).collect()
    }

    pub fn threads_by(&self, email: &str, limit: usize, staff: bool) -> Vec<&Thread> {
        let v = self
            .threads
            .values()
            .filter(|t| t.author == email && self.thread_visible(t, staff))
            .collect();
        self.sorted(v, false).into_iter().take(limit).collect()
    }

    pub fn pinned_announcements(&self, staff: bool) -> Vec<&Thread> {
        let v = self
            .threads
            .values()
            .filter(|t| t.pinned && t.cat == "announcements" && self.thread_visible(t, staff))
            .collect();
        self.sorted(v, false)
    }

    pub fn post_count_by(&self, email: &str) -> usize {
        self.posts
            .values()
            .filter(|p| {
                p.author == email
                    && !p.hidden
                    && self
                        .threads
                        .get(&p.thread)
                        .is_some_and(|t| !t.deleted && !t.hidden)
            })
            .count()
    }

    /// Case-insensitive substring search over titles and bodies of visible content. Title hits
    /// rank before body hits, then newest activity first. Returns thread ids with a snippet.
    pub fn search(&self, q: &str, staff: bool) -> Vec<(u64, String)> {
        let needle = q.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(bool, u64, u64, String)> = Vec::new();
        for t in self
            .threads
            .values()
            .filter(|t| self.thread_visible(t, staff))
        {
            let title_hit = t.title.to_lowercase().contains(&needle);
            let mut snippet = String::new();
            let mut body_hit = false;
            for p in self.thread_posts(t.id) {
                if p.hidden && !staff {
                    continue;
                }
                if p.body.to_lowercase().contains(&needle) {
                    body_hit = true;
                    snippet = md::excerpt(&p.body, 200);
                    break;
                }
            }
            if title_hit || body_hit {
                if snippet.is_empty() {
                    snippet = self
                        .thread_posts(t.id)
                        .first()
                        .map(|p| md::excerpt(&p.body, 200))
                        .unwrap_or_default();
                }
                hits.push((title_hit, t.last_activity, t.id, snippet));
            }
        }
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
        hits.into_iter().map(|(_, _, id, sn)| (id, sn)).collect()
    }

    /// Most recent change anywhere in the forum.
    pub fn last_modified(&self) -> u64 {
        self.threads
            .values()
            .map(|t| t.updated.max(t.last_activity))
            .max()
            .unwrap_or(0)
    }

    pub fn category_last_modified(&self, slug: &str) -> u64 {
        self.threads
            .values()
            .filter(|t| t.cat == slug)
            .map(|t| t.updated.max(t.last_activity))
            .max()
            .unwrap_or(0)
    }
}

/// `(threads, posts, latest activity)` of the visible content in a category.
pub fn category_stats(d: &Data, slug: &str, staff: bool) -> (usize, usize, Option<u64>) {
    let ts = d.category_threads(slug, staff);
    let posts = ts
        .iter()
        .map(|t| d.thread_posts(t.id).iter().filter(|p| !p.hidden).count())
        .sum();
    (ts.len(), posts, ts.iter().map(|t| t.last_activity).max())
}

/// Page `page` (1-based) of `total` items, `per` per page: `(offset, pages)`; `None` when the
/// page is beyond the last one.
pub fn paginate(total: usize, per: usize, page: usize) -> Option<(usize, usize)> {
    let pages = total.div_ceil(per).max(1);
    if page == 0 || page > pages {
        return None;
    }
    Some(((page - 1) * per, pages))
}

#[cfg(test)]
pub(crate) fn test_forum(admins: &[&str]) -> Forum {
    let backend = obop_storage::open("memory:").unwrap();
    Forum::open(backend, admins.iter().map(|a| a.to_string()).collect()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000;

    fn forum() -> Forum {
        let f = test_forum(&["root@example.test"]);
        f.register("ada@example.test", Some("Ada"), T0 - 10 * 86400)
            .unwrap();
        f.register("root@example.test", Some("obop-team"), T0 - 10 * 86400)
            .unwrap();
        f
    }

    #[test]
    fn categories_are_seeded_once() {
        let backend = obop_storage::open("memory:").unwrap();
        let f = Forum::open(backend.clone(), HashSet::new()).unwrap();
        assert_eq!(f.lock().categories.len(), 8);
        assert!(f.lock().category("announcements").unwrap().staff_only);
        let f2 = Forum::open(backend, HashSet::new()).unwrap();
        assert_eq!(f2.lock().categories.len(), 8);
    }

    #[test]
    fn display_names_are_derived_unique_and_never_emails() {
        let f = test_forum(&[]);
        let a = f.register("Ada.Lovelace@example.test", None, T0).unwrap();
        assert_eq!(a.name, "ada-lovelace");
        assert!(!a.name.contains('@'));
        let b = f.register("ada.lovelace@other.test", None, T0).unwrap();
        assert_eq!(b.name, "ada-lovelace2");
        let c = f.register("x@example.test", None, T0).unwrap();
        assert_eq!(c.name.len(), 3);
        // The same account registers once.
        assert_eq!(f.register("x@example.test", Some("other"), T0).unwrap(), c);
        // A derived name that imitates staff is replaced.
        let r = f.register("admin@example.test", None, T0).unwrap();
        assert!(r.name.starts_with("member"), "{}", r.name);
    }

    #[test]
    fn chosen_names_are_validated() {
        let f = test_forum(&["root@example.test"]);
        for (i, bad) in [
            "ab",
            "this-name-is-far-too-long-xx",
            "sp ace",
            "ünï",
            "obop",
            "Admin",
            "the_STAFF",
            "0b0p-mod",
            "admin1",
            "my-moderator",
        ]
        .into_iter()
        .enumerate()
        {
            let e = f.register(&format!("u{i}@example.test"), Some(bad), T0);
            // "0b0p-mod" is not reserved by the rule (digits are not letters): allow only that.
            if bad == "0b0p-mod" {
                assert!(e.is_ok(), "{bad}");
                continue;
            }
            assert_eq!(e, Err(FErr::Invalid("err.name_invalid")), "{bad}");
        }
        f.register("v@example.test", Some("Valid_Name"), T0)
            .unwrap();
        assert_eq!(
            f.register("w@example.test", Some("valid_name"), T0),
            Err(FErr::Invalid("err.name_taken"))
        );
        // Staff may use reserved names.
        f.register("root@example.test", Some("obop-team"), T0)
            .unwrap();
        assert!(
            valid_email("a@b.co")
                && !valid_email("a@b")
                && !valid_email("@b.co")
                && !valid_email("a b@c.de")
        );
    }

    #[test]
    fn thread_and_reply_flow_and_validation() {
        let f = forum();
        let id = f
            .create_thread(
                "ada@example.test",
                "general",
                "  Hello   world ",
                "First post",
                T0,
            )
            .unwrap();
        let rid = f.reply("root@example.test", id, "Welcome", T0 + 5).unwrap();
        let d = f.lock();
        assert_eq!(d.threads[&id].title, "Hello world");
        assert_eq!(d.threads[&id].last_activity, T0 + 5);
        assert_eq!(d.visible_replies(id), 1);
        assert_eq!(d.posts[&rid].thread, id);
        drop(d);
        assert_eq!(
            f.create_thread("ada@example.test", "nope", "t", "b", T0),
            Err(FErr::Invalid("err.category_unknown"))
        );
        assert_eq!(
            f.create_thread("ada@example.test", "general", "", "b", T0),
            Err(FErr::Invalid("err.title_required"))
        );
        assert_eq!(
            f.create_thread("ada@example.test", "general", &"x".repeat(141), "b", T0),
            Err(FErr::Invalid("err.title_long"))
        );
        assert_eq!(
            f.create_thread("ada@example.test", "general", "t", "  ", T0),
            Err(FErr::Invalid("err.body_required"))
        );
        assert_eq!(
            f.create_thread("ada@example.test", "general", "t", &"x".repeat(20_001), T0),
            Err(FErr::Invalid("err.body_long"))
        );
        f.create_thread(
            "ada@example.test",
            "general",
            &"x".repeat(140),
            &"y".repeat(20_000),
            T0 + 7200,
        )
        .unwrap();
        assert_eq!(
            f.create_thread("ghost@example.test", "general", "t", "b", T0),
            Err(FErr::Forbidden)
        );
        assert_eq!(
            f.reply("ada@example.test", 999, "b", T0),
            Err(FErr::NotFound)
        );
    }

    #[test]
    fn staff_only_category() {
        let f = forum();
        assert_eq!(
            f.create_thread("ada@example.test", "announcements", "t", "b", T0),
            Err(FErr::Forbidden)
        );
        f.create_thread("root@example.test", "announcements", "Release", "notes", T0)
            .unwrap();
    }

    #[test]
    fn rate_limits_per_account_per_hour() {
        let f = forum();
        for i in 0..3 {
            f.create_thread("ada@example.test", "general", &format!("t{i}"), "b", T0 + i)
                .unwrap();
        }
        assert_eq!(
            f.create_thread("ada@example.test", "general", "t4", "b", T0 + 10),
            Err(FErr::Rate("err.rate_threads"))
        );
        let tid = *f.lock().threads.keys().next().unwrap();
        for i in 0..7 {
            f.reply("ada@example.test", tid, "r", T0 + 20 + i).unwrap();
        }
        assert_eq!(
            f.reply("ada@example.test", tid, "r", T0 + 40),
            Err(FErr::Rate("err.rate_posts"))
        );
        // An hour later the window has moved on.
        f.reply("ada@example.test", tid, "r", T0 + 3700).unwrap();
        // Staff are exempt.
        for i in 0..12 {
            f.reply("root@example.test", tid, "s", T0 + 3700 + i)
                .unwrap();
        }
    }

    #[test]
    fn new_accounts_get_two_links_per_post() {
        let f = test_forum(&[]);
        f.register("new@example.test", Some("Newbie"), T0 - 60)
            .unwrap();
        let three = "https://a.test https://b.test https://c.test";
        assert_eq!(
            f.create_thread("new@example.test", "general", "t", three, T0),
            Err(FErr::Invalid("err.links"))
        );
        f.create_thread(
            "new@example.test",
            "general",
            "t",
            "https://a.test https://b.test",
            T0,
        )
        .unwrap();
        f.register("old@example.test", Some("Oldie"), T0 - 2 * 86400)
            .unwrap();
        f.create_thread("old@example.test", "general", "t", three, T0)
            .unwrap();
    }

    #[test]
    fn edits_only_by_author_inside_fifteen_minutes() {
        let f = forum();
        let id = f
            .create_thread("ada@example.test", "general", "t", "one", T0)
            .unwrap();
        let pid = f.lock().by_thread[&id][0];
        assert_eq!(
            f.edit_post("root@example.test", pid, "hax", T0 + 1),
            Err(FErr::Forbidden)
        );
        f.edit_post("ada@example.test", pid, "two", T0 + 899)
            .unwrap();
        assert_eq!(f.lock().posts[&pid].body, "two");
        assert_eq!(f.lock().posts[&pid].edited, Some(T0 + 899));
        assert_eq!(
            f.edit_post("ada@example.test", pid, "three", T0 + 901),
            Err(FErr::Invalid("err.edit_window"))
        );
    }

    #[test]
    fn moderation_and_visibility() {
        let f = forum();
        let id = f
            .create_thread(
                "ada@example.test",
                "help",
                "Secret title",
                "secret body",
                T0,
            )
            .unwrap();
        let r = f.reply("ada@example.test", id, "reply", T0 + 1).unwrap();
        assert_eq!(
            f.moderate_thread("ada@example.test", id, ModAction::Hide, T0),
            Err(FErr::Forbidden)
        );
        f.moderate_thread("root@example.test", id, ModAction::Pin, T0 + 2)
            .unwrap();
        f.moderate_thread("root@example.test", id, ModAction::Lock, T0 + 3)
            .unwrap();
        assert_eq!(
            f.reply("ada@example.test", id, "x", T0 + 4),
            Err(FErr::Forbidden)
        );
        f.moderate_thread("root@example.test", id, ModAction::Unlock, T0 + 5)
            .unwrap();
        f.moderate_post("root@example.test", r, true, T0 + 6)
            .unwrap();
        let first = f.lock().by_thread[&id][0];
        assert_eq!(
            f.moderate_post("root@example.test", first, true, T0),
            Err(FErr::Forbidden)
        );
        {
            let d = f.lock();
            assert_eq!(d.visible_replies(id), 0);
        }
        f.moderate_thread("root@example.test", id, ModAction::Hide, T0 + 7)
            .unwrap();
        {
            let d = f.lock();
            assert!(d.category_threads("help", false).is_empty());
            assert_eq!(d.category_threads("help", true).len(), 1);
            assert!(d.search("secret", false).is_empty());
            assert_eq!(d.search("secret", true).len(), 1);
            assert!(d.latest_threads(10, false).is_empty());
        }
        f.moderate_thread(
            "root@example.test",
            id,
            ModAction::Move("general".into()),
            T0 + 8,
        )
        .unwrap();
        assert_eq!(
            f.moderate_thread(
                "root@example.test",
                id,
                ModAction::Move("nope".into()),
                T0 + 8
            ),
            Err(FErr::Invalid("err.category_unknown"))
        );
        assert_eq!(f.lock().threads[&id].cat, "general");
        f.moderate_thread("root@example.test", id, ModAction::Delete, T0 + 9)
            .unwrap();
        {
            let d = f.lock();
            assert!(d.threads[&id].deleted && d.threads[&id].title.is_empty());
            assert!(
                d.thread_posts(id).iter().all(|p| p.body.is_empty()),
                "text erased"
            );
            assert!(d.search("secret", true).is_empty());
        }
        assert_eq!(
            f.moderate_thread("root@example.test", id, ModAction::Pin, T0 + 9),
            Err(FErr::Gone)
        );
        let actions: Vec<String> = f.lock().audit.iter().map(|a| a.action.clone()).collect();
        assert_eq!(
            actions,
            ["pin", "lock", "unlock", "hide", "hide", "move", "delete"]
        );
        assert_eq!(f.lock().audit[0].actor, "obop-team");
    }

    #[test]
    fn search_is_case_insensitive_over_titles_and_bodies() {
        let f = forum();
        let a = f
            .create_thread("ada@example.test", "help", "Invoice PDF layout", "how?", T0)
            .unwrap();
        let b = f
            .create_thread(
                "ada@example.test",
                "help",
                "Other",
                "my invoice is wrong",
                T0 + 100,
            )
            .unwrap();
        let d = f.lock();
        let hits: Vec<u64> = d
            .search("INVOICE", false)
            .into_iter()
            .map(|h| h.0)
            .collect();
        assert_eq!(hits, [a, b], "title hits first");
        assert!(d.search("   ", false).is_empty());
        assert!(d.search("zzz", false).is_empty());
    }

    #[test]
    fn pinned_first_then_newest_activity() {
        let f = forum();
        let a = f
            .create_thread("ada@example.test", "help", "a", "b", T0)
            .unwrap();
        let b = f
            .create_thread("ada@example.test", "help", "b", "b", T0 + 10)
            .unwrap();
        let c = f
            .create_thread("ada@example.test", "help", "c", "b", T0 + 20)
            .unwrap();
        f.moderate_thread("root@example.test", a, ModAction::Pin, T0 + 30)
            .unwrap();
        let d = f.lock();
        let order: Vec<u64> = d
            .category_threads("help", false)
            .iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(order, [a, c, b]);
    }

    #[test]
    fn pagination_math() {
        assert_eq!(paginate(0, 25, 1), Some((0, 1)));
        assert_eq!(paginate(25, 25, 1), Some((0, 1)));
        assert_eq!(paginate(26, 25, 2), Some((25, 2)));
        assert_eq!(paginate(26, 25, 3), None);
        assert_eq!(paginate(10, 25, 0), None);
        assert_eq!(paginate(61, 30, 3), Some((60, 3)));
    }

    #[test]
    fn state_persists_across_reopen() {
        let backend = obop_storage::open("memory:").unwrap();
        let f = Forum::open(backend.clone(), HashSet::new()).unwrap();
        f.register("ada@example.test", Some("Ada"), T0).unwrap();
        let id = f
            .create_thread("ada@example.test", "general", "Persist", "body", T0)
            .unwrap();
        f.reply("ada@example.test", id, "again", T0 + 1).unwrap();
        let f2 = Forum::open(backend, HashSet::new()).unwrap();
        let d = f2.lock();
        assert_eq!(d.threads[&id].title, "Persist");
        assert_eq!(d.thread_posts(id).len(), 2);
        assert_eq!(d.member_by_name("ADA").unwrap().email, "ada@example.test");
        drop(d);
        let id2 = f2
            .create_thread("ada@example.test", "general", "Second", "b", T0 + 10)
            .unwrap();
        assert!(id2 > id, "ids keep counting");
    }
}
