use aci_core::{
    EdgeKind, GraphEdge, GraphNode, GraphPartition, Language, LineColumn, NodeId, NodeKind,
    SourceFile, SourceSpan, SymbolKind,
};

pub struct PartitionBuilder<'a> {
    file: &'a SourceFile,
    partition: GraphPartition,
    file_node: NodeId,
}

impl<'a> PartitionBuilder<'a> {
    pub fn new(file: &'a SourceFile) -> Self {
        let mut partition = GraphPartition::empty(file);
        let file_name = file
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let file_node = GraphNode::deterministic(
            &file.repo_id,
            Some(&file.file_id),
            NodeKind::File,
            file.language,
            file_name,
            Some(file.path.to_string_lossy().replace('\\', "/")),
            None,
        );
        let file_node_id = file_node.id.clone();
        partition.nodes.push(file_node);
        Self {
            file,
            partition,
            file_node: file_node_id,
        }
    }

    pub fn file_node(&self) -> NodeId {
        self.file_node.clone()
    }

    pub fn add_symbol(
        &mut self,
        name: &str,
        qualified_name: &str,
        kind: SymbolKind,
        span: SourceSpan,
    ) -> NodeId {
        let node = GraphNode::deterministic(
            &self.file.repo_id,
            Some(&self.file.file_id),
            NodeKind::Symbol,
            self.file.language,
            Some(name.to_string()),
            Some(qualified_name.to_string()),
            Some(span.clone()),
        )
        .with_symbol_kind(kind);
        let id = node.id.clone();
        self.partition.nodes.push(node);
        self.add_edge(
            EdgeKind::Defines,
            self.file_node.clone(),
            id.clone(),
            Some(span),
        );
        id
    }

    pub fn add_import(&mut self, specifier: &str, span: SourceSpan) -> NodeId {
        let import_node = GraphNode::deterministic(
            &self.file.repo_id,
            Some(&self.file.file_id),
            NodeKind::Import,
            self.file.language,
            Some(specifier.to_string()),
            Some(specifier.to_string()),
            Some(span.clone()),
        );
        let import_id = import_node.id.clone();
        self.partition.nodes.push(import_node);
        let package_id = self.add_external(NodeKind::Package, specifier, None);
        self.add_edge(EdgeKind::Imports, import_id.clone(), package_id, Some(span));
        self.add_edge(
            EdgeKind::Contains,
            self.file_node.clone(),
            import_id.clone(),
            None,
        );
        import_id
    }

    pub fn add_export(&mut self, name: &str, span: SourceSpan) -> NodeId {
        let export_node = GraphNode::deterministic(
            &self.file.repo_id,
            Some(&self.file.file_id),
            NodeKind::Export,
            self.file.language,
            Some(name.to_string()),
            Some(name.to_string()),
            Some(span.clone()),
        );
        let export_id = export_node.id.clone();
        self.partition.nodes.push(export_node);
        self.add_edge(
            EdgeKind::Exports,
            self.file_node.clone(),
            export_id.clone(),
            Some(span),
        );
        export_id
    }

    pub fn add_call(&mut self, caller: NodeId, callee: &str, span: SourceSpan) {
        let callee_id =
            self.add_external(NodeKind::ExternalSymbol, callee, Some(SymbolKind::Unknown));
        self.add_edge(EdgeKind::Calls, caller, callee_id, Some(span));
    }

    pub fn add_reference(&mut self, from: NodeId, target: &str, span: SourceSpan) {
        let target_id =
            self.add_external(NodeKind::ExternalSymbol, target, Some(SymbolKind::Unknown));
        self.add_edge(EdgeKind::References, from, target_id, Some(span));
    }

    pub fn finish(self) -> GraphPartition {
        self.partition
    }

    fn add_external(
        &mut self,
        kind: NodeKind,
        name: &str,
        symbol_kind: Option<SymbolKind>,
    ) -> NodeId {
        let mut node = GraphNode::deterministic(
            &self.file.repo_id,
            None,
            kind,
            Language::Unknown,
            Some(name.to_string()),
            Some(name.to_string()),
            None,
        );
        node.symbol_kind = symbol_kind;
        let id = node.id.clone();
        if !self
            .partition
            .nodes
            .iter()
            .any(|existing| existing.id == id)
        {
            self.partition.nodes.push(node);
        }
        id
    }

    fn add_edge(&mut self, kind: EdgeKind, from: NodeId, to: NodeId, span: Option<SourceSpan>) {
        let edge = GraphEdge::deterministic(kind, &from, &to, span);
        if !self
            .partition
            .edges
            .iter()
            .any(|existing| existing.id == edge.id)
        {
            self.partition.edges.push(edge);
        }
    }
}

pub fn line_span(text: &str, line_index: usize) -> SourceSpan {
    let byte_start = text
        .lines()
        .take(line_index)
        .map(|line| line.len() + 1)
        .sum::<usize>();
    let line = text.lines().nth(line_index).unwrap_or_default();
    SourceSpan::new(
        byte_start as u32,
        (byte_start + line.len()) as u32,
        LineColumn::new(line_index as u32 + 1, 1),
        LineColumn::new(line_index as u32 + 1, line.chars().count() as u32 + 1),
    )
}

pub fn first_identifier_after<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.trim_start().strip_prefix(prefix)?.trim_start();
    read_identifier(rest)
}

pub fn read_identifier(input: &str) -> Option<&str> {
    let end = input
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
        .map(|(index, ch)| index + ch.len_utf8())
        .last()?;
    Some(&input[..end])
}

pub fn quoted_module(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    [b'"', b'\''].into_iter().find_map(|quote| {
        let start = bytes.iter().position(|byte| *byte == quote)?;
        let end = bytes[start + 1..]
            .iter()
            .position(|byte| *byte == quote)
            .map(|offset| start + 1 + offset)?;
        Some(&line[start + 1..end])
    })
}

pub fn call_identifiers(line: &str) -> impl Iterator<Item = &str> {
    line.split('(')
        .filter_map(|part| {
            let candidate = part
                .trim_end()
                .rsplit(|ch: char| {
                    !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' || ch == '.')
                })
                .next()
                .unwrap_or_default();
            candidate.rsplit('.').next()
        })
        .filter(|name| {
            !name.is_empty()
                && !matches!(
                    *name,
                    "if" | "for" | "while" | "switch" | "function" | "def" | "class" | "return"
                )
        })
}
