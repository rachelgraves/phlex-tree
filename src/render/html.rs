use crate::diff::Summary;
use crate::graph::Graph;

const TEMPLATE: &str = include_str!("template.html");

pub fn render(g: &Graph, summary: Option<&Summary>, title: &str) -> String {
    let data = super::json::render(g, summary).replace("</", "<\\/");
    TEMPLATE
        .replace("__TITLE__", &html_escape(title))
        .replace("__DATA__", &data)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
