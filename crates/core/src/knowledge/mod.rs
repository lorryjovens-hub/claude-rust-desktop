pub mod flow;
pub mod intel;
pub mod github_hub;
pub use flow::*;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub links: Vec<String>,        // IDs of linked nodes ([[wiki-links]])
    pub backlinks: Vec<String>,    // IDs of nodes linking to this
    pub created_at: String,
    pub updated_at: String,
    pub source: String,            // 'manual' | 'import' | 'chat_auto'
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: Vec<KnowledgeNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub weight: f64,
}

pub struct KnowledgeBase {
    store_dir: PathBuf,
    nodes: RwLock<HashMap<String, KnowledgeNode>>,
}

impl KnowledgeBase {
    pub fn new(store_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&store_dir).ok();
        Self { store_dir, nodes: RwLock::new(HashMap::new()) }
    }

    pub async fn load(&self) {
        {
            let mut nodes = self.nodes.write().await;
            nodes.clear();
            if let Ok(entries) = std::fs::read_dir(&self.store_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Ok(data) = std::fs::read_to_string(&path) {
                            if let Ok(node) = serde_json::from_str::<KnowledgeNode>(&data) {
                                nodes.insert(node.id.clone(), node);
                            }
                        }
                    }
                }
            }
        }
        self.rebuild_backlinks().await;
    }

    async fn rebuild_backlinks(&self) {
        let nodes = self.nodes.read().await;
        let mut backlink_map: HashMap<String, Vec<String>> = HashMap::new();
        for node in nodes.values() {
            for link in &node.links {
                backlink_map.entry(link.clone()).or_default().push(node.id.clone());
            }
        }
        drop(nodes);
        let mut nodes = self.nodes.write().await;
        for (target_id, sources) in backlink_map {
            if let Some(node) = nodes.get_mut(&target_id) {
                node.backlinks = sources;
            }
        }
    }

    pub async fn add_node(&self, node: KnowledgeNode) -> Result<(), String> {
        let file_path = self.store_dir.join(format!("{}.json", node.id));
        let data = serde_json::to_string_pretty(&node).map_err(|e| e.to_string())?;
        std::fs::write(&file_path, data).map_err(|e| e.to_string())?;
        self.nodes.write().await.insert(node.id.clone(), node);
        self.rebuild_backlinks().await;
        Ok(())
    }

    pub async fn delete_node(&self, id: &str) -> Result<(), String> {
        let file_path = self.store_dir.join(format!("{}.json", id));
        std::fs::remove_file(file_path).ok();
        self.nodes.write().await.remove(id);
        self.rebuild_backlinks().await;
        Ok(())
    }

    pub async fn get_node(&self, id: &str) -> Option<KnowledgeNode> {
        self.nodes.read().await.get(id).cloned()
    }

    pub async fn list_nodes(&self) -> Vec<KnowledgeNode> {
        self.nodes.read().await.values().cloned().collect()
    }

    pub async fn search(&self, query: &str) -> Vec<KnowledgeNode> {
        let q = query.to_lowercase();
        self.nodes.read().await.values()
            .filter(|n| n.title.to_lowercase().contains(&q)
                || n.content.to_lowercase().contains(&q)
                || n.tags.iter().any(|t| t.to_lowercase().contains(&q)))
            .cloned()
            .collect()
    }

    pub async fn get_graph(&self) -> KnowledgeGraph {
        let nodes = self.nodes.read().await;
        let node_list: Vec<KnowledgeNode> = nodes.values().cloned().collect();
        let mut edges = Vec::new();
        let id_set: HashSet<&str> = nodes.keys().map(|s| s.as_str()).collect();
        for node in nodes.values() {
            for link in &node.links {
                if id_set.contains(link.as_str()) {
                    edges.push(GraphEdge {
                        source: node.id.clone(),
                        target: link.clone(),
                        weight: 1.0,
                    });
                }
            }
        }
        KnowledgeGraph { nodes: node_list, edges }
    }

    pub async fn import_markdown(&self, file_path: &str, content: &str) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let file_name = std::path::Path::new(file_path)
            .file_stem().and_then(|s| s.to_str()).unwrap_or("untitled");

        // Extract wiki-links [[Title]]
        let mut links = Vec::new();
        for cap in regex::Regex::new(r"\[\[([^\]]+)\]\]").unwrap().captures_iter(content) {
            if let Some(m) = cap.get(1) {
                links.push(m.as_str().to_string());
            }
        }
        // Extract tags #tag
        let mut tags = Vec::new();
        for cap in regex::Regex::new(r"#([a-zA-Z0-9_\-一-鿿]+)").unwrap().captures_iter(content) {
            if let Some(m) = cap.get(1) {
                tags.push(m.as_str().to_string());
            }
        }
        let node = KnowledgeNode {
            id: id.clone(),
            title: file_name.to_string(),
            content: content.to_string(),
            tags,
            links,
            backlinks: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            source: "import".to_string(),
            file_path: Some(file_path.to_string()),
        };
        self.add_node(node).await?;
        Ok(id)
    }
}

pub use self::flow::KnowledgeFlow;
pub use self::intel::ProjectIntel;
pub use self::intel::{ProjectProfile, FusionSuggestion, SkillBlueprint};
pub use self::github_hub::GitHubHub;

pub struct FlowEngine {
    pub flow: KnowledgeFlow,
}

pub struct IntelEngine {
    pub intel: ProjectIntel,
}

