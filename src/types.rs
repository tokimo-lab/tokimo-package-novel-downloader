use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub site: String,
    pub book_id: String,
    pub title: String,
    pub author: String,
    pub latest_chapter: String,
    pub update_date: String,
    pub word_count: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookInfo {
    pub book_name: String,
    pub author: String,
    pub summary: String,
    pub cover_url: String,
    pub update_time: String,
    pub word_count: String,
    pub serial_status: String,
    pub volumes: Vec<Volume>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub volume_name: String,
    pub chapters: Vec<ChapterInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterInfo {
    pub title: String,
    pub chapter_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub title: String,
    pub content: String,
}

impl Default for BookInfo {
    fn default() -> Self {
        Self {
            book_name: String::new(),
            author: String::new(),
            summary: String::new(),
            cover_url: String::new(),
            update_time: String::new(),
            word_count: String::new(),
            serial_status: String::new(),
            volumes: Vec::new(),
        }
    }
}
