use crate::graph::{Edge, Graph, Node, Status};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn wanted(path: &str) -> bool {
    path.ends_with(".rb") || path.ends_with(".erb")
}

pub fn read_base(root: &Path, base: &str) -> Result<Vec<(String, String)>, String> {
    let sha = git(root, &["merge-base", "HEAD", base])?;
    let listing = git(root, &["ls-tree", "-r", "--name-only", &sha, "--", "app"])?;
    let paths: Vec<String> = listing
        .lines()
        .filter(|p| wanted(p))
        .map(String::from)
        .collect();

    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run git cat-file: {e}"))?;
    let mut stdin = child.stdin.take().unwrap();
    let request: String = paths.iter().map(|p| format!("{sha}:./{p}\n")).collect();
    let writer = std::thread::spawn(move || stdin.write_all(request.as_bytes()));
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    writer.join().unwrap().map_err(|e| e.to_string())?;

    parse_batch(&out.stdout, &paths)
}

fn parse_batch(bytes: &[u8], paths: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut files = Vec::with_capacity(paths.len());
    let mut pos = 0;
    for path in paths {
        let nl = bytes[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("git cat-file output ended early")?;
        let header = std::str::from_utf8(&bytes[pos..pos + nl]).unwrap_or("");
        pos += nl + 1;
        if header.ends_with("missing") {
            continue;
        }
        let size: usize = header
            .rsplit(' ')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("bad git cat-file header: {header}"))?;
        let body = String::from_utf8_lossy(&bytes[pos..pos + size]).into_owned();
        pos += size + 1;
        files.push((path.clone(), body));
    }
    Ok(files)
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub added_nodes: Vec<String>,
    pub removed_nodes: Vec<String>,
    pub added_edges: Vec<[String; 2]>,
    pub removed_edges: Vec<[String; 2]>,
}

pub fn compare(old: &Graph, new: &Graph) -> (Graph, Summary) {
    let mut summary = Summary::default();
    let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
    for (name, n) in &new.nodes {
        let mut n = n.clone();
        if !old.nodes.contains_key(name) {
            n.status = Some(Status::Added);
            summary.added_nodes.push(name.clone());
        }
        nodes.insert(name.clone(), n);
    }
    for (name, n) in &old.nodes {
        if !new.nodes.contains_key(name) {
            let mut n = n.clone();
            n.status = Some(Status::Removed);
            summary.removed_nodes.push(name.clone());
            nodes.insert(name.clone(), n);
        }
    }

    let key = |e: &Edge| (e.from.clone(), e.to.clone());
    let old_keys: BTreeSet<_> = old.edges.iter().map(key).collect();
    let new_keys: BTreeSet<_> = new.edges.iter().map(key).collect();
    let mut edges = Vec::new();
    for e in &new.edges {
        let mut e = e.clone();
        if !old_keys.contains(&key(&e)) {
            e.status = Some(Status::Added);
            summary.added_edges.push([e.from.clone(), e.to.clone()]);
        }
        edges.push(e);
    }
    for e in &old.edges {
        if !new_keys.contains(&key(e)) {
            let mut e = e.clone();
            e.status = Some(Status::Removed);
            summary.removed_edges.push([e.from.clone(), e.to.clone()]);
            edges.push(e);
        }
    }

    (Graph::from_parts(nodes, edges), summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;

    fn graph(files: &[(&str, &str)]) -> Graph {
        let scans: Vec<_> = files.iter().map(|(p, s)| scan(p, s)).collect();
        Graph::build(&scans)
    }

    #[test]
    fn finds_added_edge_and_removed_node() {
        let old = graph(&[
            ("app/components/a.rb", "class Components::A < Components::Base\nend\n"),
            ("app/components/gone.rb", "class Components::Gone < Components::Base\nend\n"),
            ("app/views/v.rb", "class Views::V < Views::Base\n  def x\n    Gone()\n  end\nend\n"),
        ]);
        let new = graph(&[
            ("app/components/a.rb", "class Components::A < Components::Base\nend\n"),
            ("app/views/v.rb", "class Views::V < Views::Base\n  def x\n    A()\n  end\nend\n"),
        ]);
        let (merged, s) = compare(&old, &new);
        assert_eq!(s.added_edges, vec![["Views::V".to_string(), "Components::A".to_string()]]);
        assert_eq!(s.removed_nodes, vec!["Components::Gone"]);
        assert_eq!(s.removed_edges.len(), 1);
        assert_eq!(merged.nodes["Components::Gone"].status, Some(Status::Removed));
        assert_eq!(merged.children("Views::V").len(), 2);
    }

    #[test]
    fn parses_cat_file_batch_output() {
        let raw = b"abc blob 5\nhello\nxyz missing\ndef blob 2\nhi\n";
        let paths = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let files = parse_batch(raw, &paths).unwrap();
        assert_eq!(files, vec![("a".into(), "hello".into()), ("c".into(), "hi".into())]);
    }
}
