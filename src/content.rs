use ammonia::Builder;
use pulldown_cmark::{Event, Options, Parser, html};

pub fn render_markdown_safe(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options)
        .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));
    let mut rendered = String::new();
    html::push_html(&mut rendered, parser);

    Builder::default().clean(&rendered).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_markdown() {
        let html = render_markdown_safe("hello **Mythenheim**");

        assert!(html.contains("<strong>Mythenheim</strong>"));
    }

    #[test]
    fn strips_script_tags() {
        let html = render_markdown_safe("<script>alert('xss')</script>safe");

        assert!(!html.contains("<script"));
        assert!(!html.contains("alert"));
    }

    #[test]
    fn strips_javascript_links() {
        let html = render_markdown_safe("[bad](javascript:alert(1))");

        assert!(!html.contains("javascript:"));
        assert!(html.contains("bad"));
    }

    #[test]
    fn strips_unsafe_image_links() {
        let html = render_markdown_safe("![bad](javascript:alert(1))");

        assert!(!html.contains("javascript:"));
        assert!(!html.contains("src="));
        assert!(html.contains("bad"));
    }
}
