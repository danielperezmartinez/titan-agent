use crate::{
    protocol::{Project, Session},
    session::{AgentCli, StateCounts},
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub session: usize,
    pub depth: usize,
    pub branch: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectGroup {
    pub project: Project,
    pub rows: Vec<TreeRow>,
    pub counts: StateCounts,
}
pub fn matches_filter(s: &Session, f: &str) -> bool {
    let f = f.trim().to_lowercase();
    f.is_empty()
        || [
            s.name.as_str(),
            s.cli.command(),
            s.cli.tag(),
            match s.cli {
                AgentCli::ClaudeCode => "Claude Code",
                AgentCli::Codex => "Codex",
            },
            s.project.name.as_str(),
            s.model.as_deref().unwrap_or(""),
        ]
        .iter()
        .any(|x| x.to_lowercase().contains(&f))
}
fn sort(s: &[Session], v: &mut [usize]) {
    v.sort_by(|a, b| {
        s[*a]
            .started_at
            .is_none()
            .cmp(&s[*b].started_at.is_none())
            .then_with(|| s[*a].started_at.cmp(&s[*b].started_at))
            .then_with(|| s[*a].id.cmp(&s[*b].id))
    })
}
pub fn build_tree(s: &[Session], filter: &str) -> Vec<ProjectGroup> {
    let included: Vec<usize> = (0..s.len())
        .filter(|&i| matches_filter(&s[i], filter))
        .collect();
    let mut by: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
    for &i in &included {
        by.entry(s[i].project.root.clone()).or_default().push(i)
    }
    let mut groups = Vec::new();
    for (_, members) in by {
        let project = s[members[0]].project.clone();
        let mut children: std::collections::HashMap<usize, Vec<usize>> = Default::default();
        let mut roots = Vec::new();
        let parent_of: std::collections::HashMap<usize, usize> = members
            .iter()
            .filter_map(|&i| {
                s[i].parent
                    .as_deref()
                    .and_then(|id| members.iter().copied().find(|&j| s[j].id == id))
                    .map(|p| (i, p))
            })
            .collect();
        let mut cyclic = std::collections::HashSet::new();
        for &start in &members {
            let mut path = Vec::new();
            let mut positions = std::collections::HashMap::new();
            let mut node = start;
            while let Some(&parent) = parent_of.get(&node) {
                if let Some(&at) = positions.get(&node) {
                    cyclic.extend(path[at..].iter().copied());
                    break;
                }
                positions.insert(node, path.len());
                path.push(node);
                node = parent;
            }
        }
        for &i in &members {
            if cyclic.contains(&i) {
                roots.push(i)
            } else if let Some(&p) = parent_of.get(&i) {
                children.entry(p).or_default().push(i)
            } else {
                roots.push(i)
            }
        }
        sort(s, &mut roots);
        for c in children.values_mut() {
            sort(s, c)
        }
        let mut rows = Vec::new();
        let mut seen = std::collections::HashSet::new();
        fn visit(
            i: usize,
            depth: usize,
            anc: &[bool],
            last: bool,
            ch: &std::collections::HashMap<usize, Vec<usize>>,
            rows: &mut Vec<TreeRow>,
            seen: &mut std::collections::HashSet<usize>,
        ) {
            if !seen.insert(i) {
                return;
            }
            let mut branch = String::new();
            if depth > 0 {
                for &a in anc {
                    branch.push_str(if a { "|  " } else { "   " })
                }
                branch.push_str(if last { "`- " } else { "|- " })
            }
            rows.push(TreeRow {
                session: i,
                depth,
                branch,
            });
            if let Some(kids) = ch.get(&i) {
                for (n, &kid) in kids.iter().enumerate() {
                    let mut next = anc.to_vec();
                    if depth > 0 {
                        next.push(!last)
                    }
                    visit(kid, depth + 1, &next, n + 1 == kids.len(), ch, rows, seen)
                }
            }
        }
        for (n, &i) in roots.iter().enumerate() {
            visit(
                i,
                0,
                &[],
                n + 1 == roots.len(),
                &children,
                &mut rows,
                &mut seen,
            )
        }
        for &i in &members {
            if !seen.contains(&i) {
                visit(i, 0, &[], true, &children, &mut rows, &mut seen)
            }
        }
        let counts = StateCounts::tally(rows.iter().map(|r| s[r.session].state));
        groups.push(ProjectGroup {
            project,
            rows,
            counts,
        });
    }
    groups.sort_by(|a, b| {
        a.project
            .name
            .to_lowercase()
            .cmp(&b.project.name.to_lowercase())
            .then_with(|| a.project.root.cmp(&b.project.root))
    });
    groups
}
