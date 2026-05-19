use crate::content::render_markdown_safe;
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

pub const MAX_CATEGORY_NAME_LEN: usize = 80;
pub const MAX_TOPIC_TITLE_LEN: usize = 160;
pub const MAX_POST_CONTENT_BYTES: usize = 65_536;
pub const DEFAULT_PAGE_SIZE: usize = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub is_locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    pub id: String,
    pub category_id: String,
    pub author_id: String,
    pub title: String,
    pub slug: String,
    pub reply_count: u32,
    pub is_locked: bool,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: String,
    pub topic_id: String,
    pub author_id: String,
    pub content_raw: String,
    pub content_html: String,
    pub revision: u32,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicDetail {
    pub topic: Topic,
    pub posts: Vec<Post>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForumError {
    InvalidCategoryName,
    InvalidTopicTitle,
    InvalidPostContent,
    CategoryNotFound,
    TopicNotFound,
    PostNotFound,
    CategoryLocked,
    TopicLocked,
    StorePoisoned,
}

#[derive(Debug, Clone)]
pub struct ForumService {
    inner: Arc<Mutex<ForumState>>,
}

#[derive(Debug, Default)]
struct ForumState {
    next_category_id: u64,
    next_topic_id: u64,
    next_post_id: u64,
    categories: HashMap<String, StoredCategory>,
    category_slug_index: HashMap<String, String>,
    topics: HashMap<String, StoredTopic>,
    topic_slug_index: HashMap<String, String>,
    posts: HashMap<String, StoredPost>,
    posts_by_topic: HashMap<String, Vec<String>>,
    topics_by_category: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
struct StoredCategory {
    id: String,
    name: String,
    slug: String,
    description: Option<String>,
    parent_id: Option<String>,
    is_locked: bool,
}

#[derive(Debug, Clone)]
struct StoredTopic {
    id: String,
    category_id: String,
    author_id: String,
    title: String,
    slug: String,
    reply_count: u32,
    is_locked: bool,
    deleted: bool,
}

#[derive(Debug, Clone)]
struct StoredPost {
    id: String,
    topic_id: String,
    author_id: String,
    content_raw: String,
    content_html: String,
    revision: u32,
    deleted: bool,
}

impl ForumService {
    pub fn new_in_memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ForumState {
                next_category_id: 1,
                next_topic_id: 1,
                next_post_id: 1,
                ..ForumState::default()
            })),
        }
    }

    pub fn create_category(
        &self,
        name: &str,
        description: Option<&str>,
        parent_id: Option<&str>,
    ) -> Result<Category, ForumError> {
        let name = clean_required_text(name, MAX_CATEGORY_NAME_LEN)
            .ok_or(ForumError::InvalidCategoryName)?;
        let description =
            description.and_then(|value| clean_optional_text(value, MAX_POST_CONTENT_BYTES));
        let mut state = self.inner.lock().map_err(|_| ForumError::StorePoisoned)?;

        if let Some(parent_id) = parent_id
            && !state.categories.contains_key(parent_id)
        {
            return Err(ForumError::CategoryNotFound);
        }

        let category_id = format!("category:{}", state.next_category_id);
        state.next_category_id += 1;
        let slug = unique_slug(&name, &state.category_slug_index);
        let category = StoredCategory {
            id: category_id.clone(),
            name,
            slug: slug.clone(),
            description,
            parent_id: parent_id.map(ToOwned::to_owned),
            is_locked: false,
        };
        let public = category.public();

        state.category_slug_index.insert(slug, category_id.clone());
        state.categories.insert(category_id, category);

        Ok(public)
    }

    pub fn list_categories(&self) -> Result<Vec<Category>, ForumError> {
        let state = self.inner.lock().map_err(|_| ForumError::StorePoisoned)?;
        let mut categories = state
            .categories
            .values()
            .map(StoredCategory::public)
            .collect::<Vec<_>>();
        categories.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(categories)
    }

    pub fn create_topic(
        &self,
        category_id: &str,
        author_id: &str,
        title: &str,
        content_raw: &str,
    ) -> Result<TopicDetail, ForumError> {
        let title =
            clean_required_text(title, MAX_TOPIC_TITLE_LEN).ok_or(ForumError::InvalidTopicTitle)?;
        let content_raw = clean_post_content(content_raw)?;
        let content_html = render_markdown_safe(&content_raw);
        let mut state = self.inner.lock().map_err(|_| ForumError::StorePoisoned)?;
        let category = state
            .categories
            .get(category_id)
            .ok_or(ForumError::CategoryNotFound)?;
        if category.is_locked {
            return Err(ForumError::CategoryLocked);
        }

        let topic_id = format!("topic:{}", state.next_topic_id);
        state.next_topic_id += 1;
        let post_id = format!("post:{}", state.next_post_id);
        state.next_post_id += 1;
        let slug = unique_topic_slug(category_id, &title, &state.topic_slug_index);
        let topic = StoredTopic {
            id: topic_id.clone(),
            category_id: category_id.to_owned(),
            author_id: author_id.to_owned(),
            title,
            slug: slug.clone(),
            reply_count: 0,
            is_locked: false,
            deleted: false,
        };
        let post = StoredPost {
            id: post_id.clone(),
            topic_id: topic_id.clone(),
            author_id: author_id.to_owned(),
            content_raw,
            content_html,
            revision: 1,
            deleted: false,
        };

        state
            .topic_slug_index
            .insert(format!("{category_id}:{slug}"), topic_id.clone());
        state
            .topics_by_category
            .entry(category_id.to_owned())
            .or_default()
            .push(topic_id.clone());
        state
            .posts_by_topic
            .entry(topic_id.clone())
            .or_default()
            .push(post_id.clone());
        state.topics.insert(topic_id, topic.clone());
        state.posts.insert(post_id, post.clone());

        Ok(TopicDetail {
            topic: topic.public(),
            posts: vec![post.public()],
        })
    }

    pub fn list_topics(
        &self,
        category_id: &str,
        page: usize,
        page_size: usize,
    ) -> Result<Vec<Topic>, ForumError> {
        let state = self.inner.lock().map_err(|_| ForumError::StorePoisoned)?;
        if !state.categories.contains_key(category_id) {
            return Err(ForumError::CategoryNotFound);
        }
        let start = page.saturating_sub(1).saturating_mul(page_size);
        let topics = state
            .topics_by_category
            .get(category_id)
            .into_iter()
            .flatten()
            .filter_map(|topic_id| state.topics.get(topic_id))
            .filter(|topic| !topic.deleted)
            .skip(start)
            .take(page_size)
            .map(StoredTopic::public)
            .collect();
        Ok(topics)
    }

    pub fn get_topic(&self, topic_id: &str) -> Result<TopicDetail, ForumError> {
        let state = self.inner.lock().map_err(|_| ForumError::StorePoisoned)?;
        let topic = state
            .topics
            .get(topic_id)
            .filter(|topic| !topic.deleted)
            .ok_or(ForumError::TopicNotFound)?;
        let posts = state
            .posts_by_topic
            .get(topic_id)
            .into_iter()
            .flatten()
            .filter_map(|post_id| state.posts.get(post_id))
            .filter(|post| !post.deleted)
            .map(StoredPost::public)
            .collect();

        Ok(TopicDetail {
            topic: topic.public(),
            posts,
        })
    }

    pub fn reply(
        &self,
        topic_id: &str,
        author_id: &str,
        content_raw: &str,
    ) -> Result<Post, ForumError> {
        let content_raw = clean_post_content(content_raw)?;
        let content_html = render_markdown_safe(&content_raw);
        let mut state = self.inner.lock().map_err(|_| ForumError::StorePoisoned)?;
        let topic = state
            .topics
            .get_mut(topic_id)
            .filter(|topic| !topic.deleted)
            .ok_or(ForumError::TopicNotFound)?;
        if topic.is_locked {
            return Err(ForumError::TopicLocked);
        }
        topic.reply_count = topic.reply_count.saturating_add(1);

        let post_id = format!("post:{}", state.next_post_id);
        state.next_post_id += 1;
        let post = StoredPost {
            id: post_id.clone(),
            topic_id: topic_id.to_owned(),
            author_id: author_id.to_owned(),
            content_raw,
            content_html,
            revision: 1,
            deleted: false,
        };
        state
            .posts_by_topic
            .entry(topic_id.to_owned())
            .or_default()
            .push(post_id.clone());
        state.posts.insert(post_id, post.clone());

        Ok(post.public())
    }
}

impl Default for ForumService {
    fn default() -> Self {
        Self::new_in_memory()
    }
}

impl StoredCategory {
    fn public(&self) -> Category {
        Category {
            id: self.id.clone(),
            name: self.name.clone(),
            slug: self.slug.clone(),
            description: self.description.clone(),
            parent_id: self.parent_id.clone(),
            is_locked: self.is_locked,
        }
    }
}

impl StoredTopic {
    fn public(&self) -> Topic {
        Topic {
            id: self.id.clone(),
            category_id: self.category_id.clone(),
            author_id: self.author_id.clone(),
            title: self.title.clone(),
            slug: self.slug.clone(),
            reply_count: self.reply_count,
            is_locked: self.is_locked,
            deleted: self.deleted,
        }
    }
}

impl StoredPost {
    fn public(&self) -> Post {
        Post {
            id: self.id.clone(),
            topic_id: self.topic_id.clone(),
            author_id: self.author_id.clone(),
            content_raw: self.content_raw.clone(),
            content_html: self.content_html.clone(),
            revision: self.revision,
            deleted: self.deleted,
        }
    }
}

impl fmt::Display for ForumError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCategoryName => formatter.write_str("invalid category name"),
            Self::InvalidTopicTitle => formatter.write_str("invalid topic title"),
            Self::InvalidPostContent => formatter.write_str("invalid post content"),
            Self::CategoryNotFound => formatter.write_str("category not found"),
            Self::TopicNotFound => formatter.write_str("topic not found"),
            Self::PostNotFound => formatter.write_str("post not found"),
            Self::CategoryLocked => formatter.write_str("category is locked"),
            Self::TopicLocked => formatter.write_str("topic is locked"),
            Self::StorePoisoned => formatter.write_str("forum store lock is poisoned"),
        }
    }
}

impl std::error::Error for ForumError {}

fn clean_required_text(value: &str, max_len: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > max_len || trimmed.contains('\0') {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn clean_optional_text(value: &str, max_len: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > max_len || trimmed.contains('\0') {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn clean_post_content(value: &str) -> Result<String, ForumError> {
    clean_required_text(value, MAX_POST_CONTENT_BYTES).ok_or(ForumError::InvalidPostContent)
}

fn unique_slug(value: &str, index: &HashMap<String, String>) -> String {
    let base = slugify(value);
    let mut slug = base.clone();
    let mut suffix = 2;
    while index.contains_key(&slug) {
        slug = format!("{base}-{suffix}");
        suffix += 1;
    }
    slug
}

fn unique_topic_slug(category_id: &str, value: &str, index: &HashMap<String, String>) -> String {
    let base = slugify(value);
    let mut slug = base.clone();
    let mut suffix = 2;
    while index.contains_key(&format!("{category_id}:{slug}")) {
        slug = format!("{base}-{suffix}");
        suffix += 1;
    }
    slug
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for byte in value.bytes() {
        let lower = byte.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            slug.push(lower as char);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "item".to_owned()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHOR: &str = "user:1";

    #[test]
    fn creates_category_topic_and_reply() {
        let forum = ForumService::new_in_memory();
        let category = forum
            .create_category("General Talk", Some("Community topics"), None)
            .unwrap();
        let topic = forum
            .create_topic(
                &category.id,
                AUTHOR,
                "Welcome to Mythenheim",
                "hello **forum**",
            )
            .unwrap();

        assert_eq!(category.slug, "general-talk");
        assert_eq!(topic.topic.slug, "welcome-to-mythenheim");
        assert!(
            topic.posts[0]
                .content_html
                .contains("<strong>forum</strong>")
        );

        let reply = forum.reply(&topic.topic.id, AUTHOR, "safe reply").unwrap();
        assert_eq!(reply.revision, 1);

        let loaded = forum.get_topic(&topic.topic.id).unwrap();
        assert_eq!(loaded.topic.reply_count, 1);
        assert_eq!(loaded.posts.len(), 2);
    }

    #[test]
    fn list_topics_paginates_by_category() {
        let forum = ForumService::new_in_memory();
        let category = forum.create_category("General", None, None).unwrap();
        for index in 0..3 {
            forum
                .create_topic(&category.id, AUTHOR, &format!("Topic {index}"), "body")
                .unwrap();
        }

        let first_page = forum.list_topics(&category.id, 1, 2).unwrap();
        let second_page = forum.list_topics(&category.id, 2, 2).unwrap();

        assert_eq!(first_page.len(), 2);
        assert_eq!(second_page.len(), 1);
    }

    #[test]
    fn slugs_are_unique() {
        let forum = ForumService::new_in_memory();
        let first = forum.create_category("General", None, None).unwrap();
        let second = forum.create_category("General", None, None).unwrap();

        assert_eq!(first.slug, "general");
        assert_eq!(second.slug, "general-2");
    }

    #[test]
    fn rejects_empty_and_oversized_content() {
        let forum = ForumService::new_in_memory();
        let category = forum.create_category("General", None, None).unwrap();

        assert!(matches!(
            forum.create_topic(&category.id, AUTHOR, "Title", "   "),
            Err(ForumError::InvalidPostContent)
        ));
        assert!(matches!(
            forum.create_topic(&category.id, AUTHOR, "", "body"),
            Err(ForumError::InvalidTopicTitle)
        ));
    }

    #[test]
    fn renders_sanitized_post_html() {
        let forum = ForumService::new_in_memory();
        let category = forum.create_category("General", None, None).unwrap();
        let topic = forum
            .create_topic(
                &category.id,
                AUTHOR,
                "Unsafe HTML",
                "<script>alert(1)</script>\n\n[bad](javascript:alert(1))",
            )
            .unwrap();

        let html = &topic.posts[0].content_html;
        assert!(!html.contains("<script"));
        assert!(!html.contains("javascript:"));
        assert!(html.contains("bad"));
    }
}
