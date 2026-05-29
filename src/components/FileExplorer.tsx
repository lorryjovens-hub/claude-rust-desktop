import { getErrorMessage } from '../utils/errorHelpers';
import React, { useState, useEffect, useCallback } from 'react';
import { Folder, FolderOpen, File, FileText, ChevronRight, ChevronDown, Save, X } from 'lucide-react';
import { getFileSystemTree, readFileContent, writeFileContent, type FsFileNode } from '../api';

const CODE_EXTENSIONS = new Set(['ts', 'tsx', 'js', 'jsx', 'rs', 'py', 'go', 'java', 'c', 'cpp', 'h', 'css', 'scss', 'html', 'json', 'yaml', 'yml', 'toml', 'md', 'sh', 'bash', 'sql', 'xml', 'svg', 'graphql']);
const IGNORED_DIRS = new Set(['.git', 'node_modules', 'dist', 'build', '.next', '.nuxt', '__pycache__', '.venv', 'target', '.cargo', '.idea', '.vscode']);

function getFileIcon(name: string, isDir: boolean) {
  if (isDir) return Folder;
  const ext = name.split('.').pop()?.toLowerCase() || '';
  if (CODE_EXTENSIONS.has(ext)) return FileText;
  return File;
}

interface TreeNodeProps {
  node: FsFileNode;
  depth: number;
  selectedPath: string | null;
  onSelect: (node: FsFileNode) => void;
  expandedPaths: Set<string>;
  onToggle: (path: string) => void;
}

const TreeNode = ({ node, depth, selectedPath, onSelect, expandedPaths, onToggle }: TreeNodeProps) => {
  const isExpanded = expandedPaths.has(node.path);
  const isSelected = selectedPath === node.path;
  const hasChildren = node.is_dir && node.children && node.children.length > 0;

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (node.is_dir) {
      onToggle(node.path);
    } else {
      onSelect(node);
    }
  };

  const Icon = getFileIcon(node.name, node.is_dir);

  return (
    <div>
      <div
        className={`flex items-center gap-1 py-0.5 pr-2 rounded cursor-pointer select-none transition-colors ${
          isSelected
            ? 'bg-blue-500/20 text-blue-400'
            : 'hover:bg-white/5 text-neutral-300'
        }`}
        style={{ paddingLeft: `${depth * 16 + 4}px` }}
        onClick={handleClick}
      >
        <span className="w-4 h-4 flex-shrink-0 flex items-center justify-center">
          {node.is_dir ? (
            hasChildren ? (
              isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />
            ) : null
          ) : null}
        </span>
        <Icon size={14} className="flex-shrink-0" />
        <span className="text-[12px] truncate leading-none">{node.name}</span>
      </div>
      {node.is_dir && isExpanded && node.children && (
        <div>
          {node.children
            .filter((child) => !child.is_dir || !IGNORED_DIRS.has(child.name))
            .map((child) => (
              <TreeNode
                key={child.path}
                node={child}
                depth={depth + 1}
                selectedPath={selectedPath}
                onSelect={onSelect}
                expandedPaths={expandedPaths}
                onToggle={onToggle}
              />
            ))}
        </div>
      )}
    </div>
  );
};

interface CodeViewerProps {
  filePath: string;
  content: string;
  onChange: (content: string) => void;
  onSave: () => void;
  onClose: () => void;
  isSaving: boolean;
}

const CodeViewer = ({ filePath, content, onChange, onSave, onClose, isSaving }: CodeViewerProps) => {
  const fileName = filePath.split(/[/\\]/).pop() || filePath;

  return (
    <div className="flex flex-col h-full bg-[#1e1e1e]">
      <div className="flex items-center justify-between px-3 py-2 border-b border-white/10">
        <div className="flex items-center gap-2 min-w-0">
          <FileText size={14} className="text-neutral-400 flex-shrink-0" />
          <span className="text-[13px] text-neutral-300 font-medium truncate">{fileName}</span>
        </div>
        <div className="flex items-center gap-1 flex-shrink-0">
          <button
            onClick={onSave}
            disabled={isSaving}
            className="flex items-center gap-1 px-2 py-1 text-[11px] text-neutral-300 hover:bg-white/10 rounded transition-colors disabled:opacity-50"
          >
            <Save size={12} />
            Save
          </button>
          <button
            onClick={onClose}
            className="flex items-center gap-1 px-2 py-1 text-[11px] text-neutral-300 hover:bg-white/10 rounded transition-colors"
          >
            <X size={12} />
            Close
          </button>
        </div>
      </div>
      <textarea
        value={content}
        onChange={(e) => onChange(e.target.value)}
        className="flex-1 bg-transparent text-[13px] text-neutral-200 p-3 font-mono resize-none outline-none leading-relaxed overflow-auto"
        spellCheck={false}
      />
      <div className="px-3 py-1 border-t border-white/5 text-[10px] text-neutral-500">
        {filePath}
      </div>
    </div>
  );
};

const FileExplorer: React.FC = () => {
  const [tree, setTree] = useState<FsFileNode[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [selectedNode, setSelectedNode] = useState<FsFileNode | null>(null);
  const [fileContent, setFileContent] = useState('');
  const [isReading, setIsReading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [currentPath, setCurrentPath] = useState('');

  const loadTree = useCallback(async (dirPath?: string) => {
    setLoading(true);
    setError(null);
    try {
      const res = await getFileSystemTree(dirPath);
      setTree(res.tree);
      setCurrentPath(res.path);
      if (res.tree.length > 0) {
        const firstDir = res.tree.find((n) => n.is_dir);
        if (firstDir) {
          setExpandedPaths(new Set([firstDir.path]));
        }
      }
    } catch (e: unknown) {
      setError(getErrorMessage(e) || 'Failed to load file tree');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadTree();
  }, [loadTree]);

  const handleSelect = async (node: FsFileNode) => {
    if (node.is_dir) return;
    setSelectedNode(node);
    setIsReading(true);
    try {
      const res = await readFileContent(node.path);
      setFileContent(res.content);
    } catch (e: unknown) {
      setFileContent(`Error reading file: ${getErrorMessage(e)}`);
    } finally {
      setIsReading(false);
    }
  };

  const handleSave = async () => {
    if (!selectedNode) return;
    setIsSaving(true);
    try {
      await writeFileContent(selectedNode.path, fileContent);
    } catch (e: unknown) {
      alert(`Failed to save: ${getErrorMessage(e)}`);
    } finally {
      setIsSaving(false);
    }
  };

  const handleCloseViewer = () => {
    setSelectedNode(null);
    setFileContent('');
  };

  const handleToggle = (path: string) => {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  return (
    <div className="flex flex-col h-full text-[13px]">
      <div className="flex items-center justify-between px-3 py-2 border-b border-white/10">
        <span className="text-[11px] font-semibold text-neutral-400 uppercase tracking-wide">
          Explorer
        </span>
        <button
          onClick={() => loadTree()}
          className="text-[11px] text-neutral-400 hover:text-neutral-200 transition-colors px-2 py-0.5 rounded hover:bg-white/5"
        >
          Refresh
        </button>
      </div>

      <div className="flex-1 flex min-h-0">
        <div className={`flex-1 overflow-y-auto overflow-x-hidden py-1 ${selectedNode ? 'w-1/2 border-r border-white/10' : 'w-full'}`}>
          {loading ? (
            <div className="flex items-center justify-center py-8 text-neutral-500 text-[12px]">
              Loading...
            </div>
          ) : error ? (
            <div className="px-3 py-4 text-red-400 text-[12px]">{error}</div>
          ) : tree.length === 0 ? (
            <div className="px-3 py-4 text-neutral-500 text-[12px]">No files found</div>
          ) : (
            tree
              .filter((node) => !node.is_dir || !IGNORED_DIRS.has(node.name))
              .map((node) => (
                <TreeNode
                  key={node.path}
                  node={node}
                  depth={0}
                  selectedPath={selectedNode?.path || null}
                  onSelect={handleSelect}
                  expandedPaths={expandedPaths}
                  onToggle={handleToggle}
                />
              ))
          )}
        </div>

        {selectedNode && (
          <div className="flex-1 min-w-0">
            <CodeViewer
              filePath={selectedNode.path}
              content={fileContent}
              onChange={setFileContent}
              onSave={handleSave}
              onClose={handleCloseViewer}
              isSaving={isSaving}
            />
          </div>
        )}
      </div>
    </div>
  );
};

export default FileExplorer;
