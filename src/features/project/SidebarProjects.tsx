import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type MouseEvent,
} from "react";
import { createPortal } from "react-dom";
import { useLocation, useNavigate } from "react-router-dom";
import {
  Archive,
  ArrowUpRight,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Eye,
  Folder,
  FolderMinus,
  FolderOpen,
  FolderSymlink,
  MessageCircle,
  MoreHorizontal,
  Pencil,
  Pin,
  PinOff,
  Plus,
  Settings,
  SquarePen,
  X,
} from "lucide-react";
import {
  createProjectConversation,
  deleteProject,
  getConversation,
  getProjects,
  Project,
  updateConversation,
} from "../../api";
import { useWorkspaceModeStore } from "../../stores/useWorkspaceModeStore";
import { tauriAPI } from "../../utils/tauriAPI";
import CreateProjectDialog from "./CreateProjectDialog";
import EditProjectDialog from "./EditProjectDialog";
import TrustProjectDialog from "./TrustProjectDialog";
import {
  chatTitle as getChatTitle,
  SidebarChat,
  useSidebarPreferences,
  visibleChats,
} from "./sidebarModel";
import "./sidebarProjects.css";
import { useDialogFocus } from "./useDialogFocus";
import { useI18n } from "../../hooks/useI18n";

type Popup = {
  kind: "project" | "chat" | "details";
  id: string;
  left: number;
  top: number;
};
type MenuItem = {
  label: string;
  icon: ReactNode;
  action?: () => void;
  shortcut?: string;
  children?: MenuItem[];
  separator?: boolean;
  disabled?: boolean;
};

function Menu({ items, close }: { items: MenuItem[]; close: () => void }) {
  const [submenu, setSubmenu] = useState<{
    index: number;
    left: number;
    top: number;
  } | null>(null);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    root.current
      ?.querySelector<HTMLButtonElement>("button:not(:disabled)")
      ?.focus({ preventScroll: true });
    return () => {
      if (previous?.isConnected) previous.focus({ preventScroll: true });
    };
  }, []);
  const openSubmenu = (index: number, button: HTMLElement) => {
    const rect = button.getBoundingClientRect();
    const width = Math.min(260, window.innerWidth - 16);
    setSubmenu({
      index,
      left:
        rect.right + width + 8 <= window.innerWidth
          ? rect.right + 4
          : Math.max(8, rect.left - width - 4),
      top: Math.max(
        8,
        Math.min(
          rect.top,
          window.innerHeight -
            Math.min(400, (items[index].children?.length || 0) * 30 + 12) -
            8,
        ),
      ),
    });
  };
  return (
    <div
      ref={root}
      role="menu"
      className="project-popup-menu"
      onKeyDown={(event) => {
        if (event.key === "Escape" || event.key === "ArrowLeft") {
          event.stopPropagation();
          if (submenu) setSubmenu(null);
          else close();
        }
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          const buttons = [
            ...event.currentTarget.querySelectorAll<HTMLButtonElement>(
              ":scope > button:not(:disabled)",
            ),
          ];
          const index = buttons.indexOf(
            document.activeElement as HTMLButtonElement,
          );
          buttons[
            (index +
              (event.key === "ArrowDown" ? 1 : buttons.length - 1) +
              buttons.length) %
              buttons.length
          ]?.focus();
        }
      }}
    >
      {items.map((item, index) => (
        <button
          key={item.label}
          type="button"
          role="menuitem"
          disabled={item.disabled}
          aria-haspopup={item.children ? "menu" : undefined}
          aria-expanded={item.children ? submenu?.index === index : undefined}
          title={item.label}
          className={`project-menu-item ${item.separator ? "project-menu-separator" : ""}`}
          onMouseEnter={(event) =>
            item.children
              ? openSubmenu(index, event.currentTarget)
              : setSubmenu(null)
          }
          onKeyDown={(event) => {
            if (event.key === "ArrowRight" && item.children) {
              event.preventDefault();
              openSubmenu(index, event.currentTarget);
            }
          }}
          onClick={(event) =>
            item.children
              ? openSubmenu(index, event.currentTarget)
              : item.action?.()
          }
        >
          {item.icon}
          <span className="min-w-0 flex-1 truncate text-left">
            {item.label}
          </span>
          {item.shortcut && (
            <span className="shrink-0 text-[11px] text-claude-textSecondary">
              {item.shortcut}
            </span>
          )}
          {item.children && <ChevronRight size={13} />}
        </button>
      ))}
      {submenu && (
        <div
          className="fixed z-[192] w-[260px] max-w-[calc(100vw-16px)]"
          style={{ left: submenu.left, top: submenu.top }}
        >
          <Menu
            items={items[submenu.index].children || []}
            close={() => setSubmenu(null)}
          />
        </div>
      )}
    </div>
  );
}

export default function SidebarProjects({
  chats,
  onOpenChat,
  streamingIds,
}: {
  chats: SidebarChat[];
  onOpenChat: (id: string) => void;
  streamingIds: Set<string>;
}) {
  const navigate = useNavigate();
  const { t } = useI18n();
  const chatTitle = (chat: SidebarChat) => getChatTitle(chat, t("common.newChat"));
  const location = useLocation();
  const mode = useWorkspaceModeStore((state) => state.mode);
  const recentsMode = mode === 'code' ? 'code' : 'chat';
  const recentsCollapsed = useWorkspaceModeStore(
    (state) => state.recentsCollapsed[recentsMode],
  );
  const toggleRecents = useWorkspaceModeStore((state) => state.toggleRecents);
  const preferences = useSidebarPreferences();
  const [projects, setProjects] = useState<Project[]>([]);
  const [popup, setPopup] = useState<Popup | null>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Project | null>(null);
  const [removing, setRemoving] = useState<Project | null>(null);
  const [renaming, setRenaming] = useState<SidebarChat | null>(null);
  const [name, setName] = useState("");
  const dialogRef = useDialogFocus(!!removing || !!renaming);
  const [trust, setTrust] = useState<{
    project: Project;
    action: () => Promise<void>;
  } | null>(null);
  const [expandedLists, setExpandedLists] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const popupRef = useRef<HTMLDivElement>(null);
  const popupAnchor = useRef<HTMLElement | null>(null);
  const hoverTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const busyRef = useRef(false);
  // Projects span workspace modes; Home combines Chat, Work, and Study sessions.
  const currentChats = visibleChats(chats, mode);
  const projectChats = visibleChats(chats);
  const activeId = location.pathname.match(/^\/chat\/([^/]+)/)?.[1];
  const pinned = currentChats.filter((chat) => chat.pinned);
  const recent = currentChats.filter((chat) => !chat.pinned);
  const sortedProjects = [...projects].sort(
    (a, b) =>
      Number(preferences.pinnedProjects.includes(b.id)) -
      Number(preferences.pinnedProjects.includes(a.id)),
  );

  useEffect(() => {
    let disposed = false;
    const refresh = () =>
      getProjects()
        .then((list) => {
          if (!disposed)
            setProjects(list.filter((project) => !project.is_archived));
        })
        .catch((error) => {
          if (!disposed) setError(String(error));
        });
    void refresh();
    window.addEventListener("projectsChanged", refresh);
    window.addEventListener("focus", refresh);
    return () => {
      disposed = true;
      window.removeEventListener("projectsChanged", refresh);
      window.removeEventListener("focus", refresh);
      if (hoverTimer.current) clearTimeout(hoverTimer.current);
    };
  }, []);
  useEffect(() => {
    setPopup(null);
  }, [mode, location.pathname]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(""), 2500);
    return () => clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    if (!popup) return;
    const outside = (event: Event) => {
      if (!popupRef.current?.contains(event.target as Node)) setPopup(null);
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPopup(null);
    };
    const resize = () => setPopup(null);
    document.addEventListener("pointerdown", outside);
    const scroll = (event: Event) => {
      if (
        popupRef.current?.contains(event.target as Node) ||
        !popupAnchor.current
      )
        return;
      const rect = popupAnchor.current.getBoundingClientRect();
      const width = popup.kind === "details" ? 310 : 260;
      setPopup((current) =>
        current
          ? {
              ...current,
              left: Math.max(
                8,
                Math.min(
                  current.kind === "details" ? rect.right + 4 : rect.left,
                  window.innerWidth - width - 8,
                ),
              ),
              top: Math.max(
                8,
                Math.min(
                  current.kind === "details" ? rect.top : rect.bottom + 4,
                  window.innerHeight -
                    (current.kind === "chat" ? 262 : 230) -
                    8,
                ),
              ),
            }
          : null,
      );
    };
    document.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", resize);
    window.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("scroll", scroll, true);
      window.removeEventListener("resize", resize);
      window.removeEventListener("keydown", key);
    };
  }, [popup]);

  const perform = async (action: () => Promise<void>) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError("");
    setPopup(null);
    try {
      await action();
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };
  const patchChat = async (chat: SidebarChat, data: object) => {
    await updateConversation(chat.id, data);
    window.dispatchEvent(new Event("conversationUpdated"));
  };
  const togglePin = (chat: SidebarChat) =>
    perform(() => patchChat(chat, { pinned: !chat.pinned }));
  const archive = (chat: SidebarChat) =>
    perform(async () => {
      await patchChat(chat, { archived: true });
      if (activeId === chat.id) navigate("/");
    });
  const rename = (chat: SidebarChat) => {
    setPopup(null);
    setRenaming(chat);
    setName(chatTitle(chat));
  };
  const copy = async (text: string) => {
    if (tauriAPI.isTauri) {
      const { writeText } = await import(
        "@tauri-apps/plugin-clipboard-manager"
      );
      await writeText(text);
    } else await navigator.clipboard.writeText(text);
    setNotice(t("sidebar.copied"));
  };
  const copyLink = (chat: SidebarChat) =>
    perform(() =>
      copy(
        `${window.location.href.split("#")[0]}#/chat/${encodeURIComponent(chat.id)}`,
      ),
    );
  const copyMarkdown = (chat: SidebarChat) =>
    perform(async () => {
      const conversation = await getConversation(chat.id);
      const content = (value: unknown): string => {
        if (typeof value === "string") {
          try {
            const parsed = JSON.parse(value);
            if (Array.isArray(parsed)) return content(parsed);
          } catch {
            /* Plain text message. */
          }
          return value;
        }
        if (Array.isArray(value))
          return value
            .map((block) =>
              typeof block === "string" ? block : block?.text || "",
            )
            .filter(Boolean)
            .join("\n\n");
        return "";
      };
      await copy(
        [
          `# ${chatTitle(chat)}`,
          ...(conversation.messages || []).map(
            (message: { role: string; content: unknown }) =>
              `## ${message.role}\n\n${content(message.content)}`,
          ),
        ].join("\n\n"),
      );
    });
  const pathFor = (chat: SidebarChat) =>
    chat.workspace_path ||
    projects.find((project) => project.id === chat.project_id)?.workspace_path;
  const openFolder = (path?: string | null) =>
    perform(async () => {
      if (!path)
        throw new Error(t("sidebar.noDirectory"));
      if (!tauriAPI.isTauri)
        throw new Error(t("sidebar.explorerDesktopOnly"));
      await tauriAPI.openFolder(path);
    });
  const withTrust = (project: Project, action: () => Promise<void>) => {
    setPopup(null);
    if (project.folders?.some((folder) => !folder.trusted))
      setTrust({ project, action });
    else void perform(action);
  };
  const newChat = (project: Project) =>
    withTrust(project, async () => {
      const chat = await createProjectConversation(project.id);
      window.dispatchEvent(new Event("conversationCreated"));
      onOpenChat(chat.id);
    });
  const moveChat = (chat: SidebarChat, project: Project) =>
    withTrust(project, async () => {
      await patchChat(chat, { project_id: project.id });
      if (preferences.collapsedProjects.includes(project.id))
        preferences.toggle("collapsedProjects", project.id);
    });
  const markUnread = (chat: SidebarChat) => {
    preferences.toggle("unreadChats", chat.id);
    setPopup(null);
  };
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if (
        editing ||
        removing ||
        renaming ||
        trust ||
        creating ||
        busy ||
        !(event.ctrlKey || event.metaKey)
      )
        return;
      const target = event.target;
      if (target instanceof Element && target.closest('input, textarea, [contenteditable="true"]')) return;
      const chat = chats.find(
        (chat) => chat.id === (popup?.kind === "chat" ? popup.id : activeId),
      );
      if (!chat) return;
      const key = event.key.toLowerCase();
      if (event.altKey && key === "r") {
        event.preventDefault();
        rename(chat);
      }
      if (event.altKey && key === "p") {
        event.preventDefault();
        void togglePin(chat);
      }
      if (event.altKey && key === "l") {
        event.preventDefault();
        void copyLink(chat);
      }
      if (event.shiftKey && key === "u") {
        event.preventDefault();
        markUnread(chat);
      }
      if (event.shiftKey && key === "a") {
        event.preventDefault();
        void archive(chat);
      }
      if (event.shiftKey && key === "c" && pathFor(chat)) {
        event.preventDefault();
        void perform(() => copy(pathFor(chat)!));
      }
    };
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  });

  const stopHover = () => {
    if (hoverTimer.current) clearTimeout(hoverTimer.current);
  };
  const showPopup = (element: HTMLElement, kind: Popup["kind"], id: string) => {
    stopHover();
    popupAnchor.current = element;
    const rect = element.getBoundingClientRect();
    const width = kind === "details" ? 310 : 260;
    setPopup({
      kind,
      id,
      left: Math.max(
        8,
        Math.min(
          kind === "details" ? rect.right + 4 : rect.left,
          window.innerWidth - width - 8,
        ),
      ),
      top: Math.max(
        8,
        Math.min(
          kind === "details" ? rect.top : rect.bottom + 4,
          window.innerHeight - (kind === "chat" ? 262 : 230) - 8,
        ),
      ),
    });
  };
  const menuClick = (
    event: MouseEvent<HTMLElement>,
    kind: Popup["kind"],
    id: string,
  ) => {
    event.stopPropagation();
    event.preventDefault();
    showPopup(event.currentTarget, kind, id);
  };
  const chatRow = (chat: SidebarChat, nested = false) => (
    <div
      key={chat.id}
      data-session-id={chat.id}
      className={`project-sidebar-row group ${nested ? "project-session-row" : ""} ${activeId === chat.id || popup?.id === chat.id ? "bg-claude-hover" : ""}`}
    >
      <button
        type="button"
        aria-current={activeId === chat.id ? "page" : undefined}
        title={chatTitle(chat)}
        onContextMenu={(event) => menuClick(event, "chat", chat.id)}
        onClick={() => {
          preferences.markRead(chat.id);
          onOpenChat(chat.id);
        }}
        className={`flex min-w-0 flex-1 items-center gap-2 py-1.5 text-left ${preferences.unreadChats.includes(chat.id) ? "font-semibold" : ""}`}
      >
        {(streamingIds.has(chat.id) ||
          preferences.unreadChats.includes(chat.id)) && (
          <span
            aria-label={streamingIds.has(chat.id) ? t("sidebar.running") : t("sidebar.unread")}
            className={`h-1.5 w-1.5 shrink-0 rounded-full bg-claude-text ${streamingIds.has(chat.id) ? "animate-pulse" : ""}`}
          />
        )}
        <span className="truncate">{chatTitle(chat)}</span>
      </button>
      <div className="project-row-actions">
        <button
          type="button"
          aria-label={t("sidebar.chatOptions", { name: chatTitle(chat) })}
          onClick={(event) => menuClick(event, "chat", chat.id)}
        >
          <MoreHorizontal size={14} />
        </button>
        {chat.pinned && (
          <button
            type="button"
            disabled={busy}
            title={t("sidebar.unpinChat")}
            aria-label={t("sidebar.unpinNamed", { name: chatTitle(chat) })}
            onClick={() => void togglePin(chat)}
          >
            <Pin size={13} fill="currentColor" />
          </button>
        )}
      </div>
    </div>
  );
  const chatList = (list: SidebarChat[], key: string, nested = false) => (
    <>
      {(expandedLists.includes(key) ? list : list.slice(0, 5)).map((chat) =>
        chatRow(chat, nested),
      )}
      {list.length > 5 && (
        <button
          type="button"
          className={`py-1.5 text-[13px] text-claude-textSecondary hover:text-claude-text ${nested ? "pl-8" : "pl-2"}`}
          onClick={() =>
            setExpandedLists((previous) =>
              previous.includes(key)
                ? previous.filter((id) => id !== key)
                : [...previous, key],
            )
          }
        >
          {expandedLists.includes(key) ? t("sidebar.showLess") : t("sidebar.showMore")}
        </button>
      )}
    </>
  );

  const projectMenu = (project: Project): MenuItem[] => [
    {
      label: preferences.pinnedProjects.includes(project.id) ? t("sidebar.unpin") : t("sidebar.pin"),
      icon: <Pin size={15} />,
      action: () => {
        preferences.toggle("pinnedProjects", project.id);
        setPopup(null);
      },
    },
    {
      label: t("sidebar.edit"),
      icon: <Settings size={15} />,
      action: () => {
        setEditing(project);
        setPopup(null);
      },
    },
    {
      label: t("sidebar.openExplorer"),
      icon: <FolderOpen size={15} />,
      separator: true,
      action: () => void openFolder(project.workspace_path),
      disabled: !project.workspace_path,
    },
    {
      label: t("sidebar.archiveChats"),
      icon: <Archive size={15} />,
      separator: true,
      action: () =>
        void perform(async () => {
          for (const chat of chats.filter(
            (chat) => chat.project_id === project.id && !chat.archived,
          ))
            await patchChat(chat, { archived: true });
          if (
            chats.some(
              (chat) => chat.id === activeId && chat.project_id === project.id,
            )
          )
            navigate("/");
        }),
    },
    {
      label: t("sidebar.removeProject"),
      icon: <X size={15} />,
      separator: true,
      action: () => {
        setRemoving(project);
        setPopup(null);
      },
    },
  ];
  const chatMenu = (chat: SidebarChat): MenuItem[] => [
    {
      label: t("common.rename"),
      icon: <Pencil size={15} />,
      shortcut: "Ctrl+Alt+R",
      action: () => rename(chat),
    },
    {
      label: chat.pinned ? t("sidebar.unpin") : t("sidebar.pin"),
      icon: chat.pinned ? <PinOff size={15} /> : <Pin size={15} />,
      shortcut: "Ctrl+Alt+P",
      action: () => void togglePin(chat),
    },
    {
      label: preferences.unreadChats.includes(chat.id)
        ? t("sidebar.markRead")
        : t("sidebar.markUnread"),
      icon: <Eye size={15} />,
      shortcut: "Ctrl+Shift+U",
      action: () => markUnread(chat),
    },
    {
      label: t("sidebar.project"),
      icon: <FolderSymlink size={15} />,
      children: [
        ...projects.map((project) => ({
          label: project.name,
          icon:
            project.id === chat.project_id ? (
              <Check size={15} />
            ) : (
              <Folder size={15} />
            ),
          action: () => moveChat(chat, project),
          disabled: project.id === chat.project_id,
        })),
        ...(chat.project_id
          ? [
              {
                label: t("sidebar.removeFrom", { name: projects.find((project) => project.id === chat.project_id)?.name || t("sidebar.project") }),
                icon: <FolderMinus size={15} />,
                separator: true,
                action: () =>
                  void perform(() => patchChat(chat, { project_id: null })),
              },
            ]
          : []),
      ],
    },
    {
      label: t("sidebar.copy"),
      icon: <Copy size={15} />,
      separator: true,
      children: [
        {
          label: t("sidebar.copyDeeplink"),
          icon: <Copy size={15} />,
          shortcut: "Ctrl+Alt+L",
          action: () => void copyLink(chat),
        },
        {
          label: t("sidebar.copyMarkdown"),
          icon: <Copy size={15} />,
          action: () => void copyMarkdown(chat),
        },
        {
          label: t("sidebar.copyDirectory"),
          icon: <Copy size={15} />,
          shortcut: "Ctrl+Shift+C",
          disabled: !pathFor(chat),
          action: () => void perform(() => copy(pathFor(chat)!)),
        },
      ],
    },
    {
      label: t("sidebar.openIn"),
      icon: <ArrowUpRight size={15} />,
      separator: true,
      children: [
        {
          label: t("sidebar.fileExplorer"),
          icon: <FolderOpen size={15} />,
          disabled: !pathFor(chat),
          action: () => void openFolder(pathFor(chat)),
        },
      ],
    },
    {
      label: t("sidebar.archive"),
      icon: <Archive size={15} />,
      separator: true,
      shortcut: "Ctrl+Shift+A",
      action: () => void archive(chat),
    },
  ];
  const popupProject = projects.find((project) => project.id === popup?.id);
  const popupChat = chats.find((chat) => chat.id === popup?.id);

  return (
    <div className="project-sidebar-tree" aria-busy={busy}>
      {!!pinned.length && (
        <section aria-label={t("sidebar.pinnedChats")} className="mb-4">
          <button
            type="button"
            className="project-section-heading"
            aria-expanded={!preferences.pinnedCollapsed}
            onClick={() => preferences.collapse("pinnedCollapsed")}
          >
            {t("sidebar.pinned")}
            <ChevronDown
              size={13}
              className={preferences.pinnedCollapsed ? "-rotate-90" : ""}
            />
          </button>
          {!preferences.pinnedCollapsed && chatList(pinned, "pinned")}
        </section>
      )}
      <section aria-label={t("sidebar.projects")}>
        <div className="flex items-center">
          <button
            type="button"
            className="project-section-heading flex-1"
            aria-expanded={!preferences.projectsCollapsed}
            onClick={() => preferences.collapse("projectsCollapsed")}
          >
            {t("sidebar.projects")}
            <ChevronDown
              size={13}
              className={preferences.projectsCollapsed ? "-rotate-90" : ""}
            />
          </button>
          <button
            type="button"
            onClick={() => navigate("/projects")}
            className="mr-1 rounded px-1 py-1 text-[11px] text-claude-textSecondary hover:bg-claude-hover hover:text-claude-text"
          >
            {t("sidebar.allProjects")}
          </button>
          <button
            type="button"
            aria-label={t("sidebar.createProject")}
            title={t("sidebar.createProject")}
            className="rounded p-1 text-claude-textSecondary hover:bg-claude-hover"
            onClick={() => setCreating(true)}
          >
            <Plus size={14} />
          </button>
        </div>
        {!preferences.projectsCollapsed && (
          <div data-project-list>
            {sortedProjects.map((project) => {
              const expanded = !preferences.collapsedProjects.includes(
                project.id,
              );
              const sessions = projectChats.filter(
                (chat) => chat.project_id === project.id,
              );
              return (
                <div key={project.id} className="mb-1">
                  <div
                    data-project-id={project.id}
                    className={`project-sidebar-row group ${popup?.id === project.id ? "bg-claude-hover" : ""}`}
                    onMouseEnter={(event) => {
                      if (popup && popup.kind !== "details") return;
                      const element = event.currentTarget;
                      stopHover();
                      hoverTimer.current = setTimeout(
                        () => showPopup(element, "details", project.id),
                        550,
                      );
                    }}
                    onMouseLeave={() => {
                      stopHover();
                      hoverTimer.current = setTimeout(
                        () =>
                          setPopup((current) =>
                            current?.kind === "details" ? null : current,
                          ),
                        220,
                      );
                    }}
                  >
                    <button
                      type="button"
                      aria-expanded={expanded}
                      aria-label={project.name}
                      onContextMenu={(event) =>
                        menuClick(event, "project", project.id)
                      }
                      onClick={() => {
                        stopHover();
                        setPopup(null);
                        preferences.toggle("collapsedProjects", project.id);
                      }}
                      className="flex min-w-0 flex-1 items-center gap-2 py-1.5 text-left"
                    >
                      {expanded ? (
                        <FolderOpen size={15} className="shrink-0" />
                      ) : (
                        <Folder size={15} className="shrink-0" />
                      )}
                      <span className="truncate">{project.name}</span>
                      {preferences.pinnedProjects.includes(project.id) && (
                        <Pin
                          size={11}
                          className="shrink-0 text-claude-textSecondary"
                        />
                      )}
                    </button>
                    <div className="project-row-actions">
                      <button
                        type="button"
                        aria-label={t("sidebar.projectOptions", { name: project.name })}
                        onClick={(event) =>
                          menuClick(event, "project", project.id)
                        }
                      >
                        <MoreHorizontal size={14} />
                      </button>
                      <button
                        type="button"
                        aria-label={t("sidebar.newChatIn", { name: project.name })}
                        title={t("sidebar.newChat")}
                        disabled={busy}
                        onClick={() => {
                          stopHover();
                          newChat(project);
                        }}
                      >
                        <SquarePen size={14} />
                      </button>
                    </div>
                  </div>
                  {expanded && (sessions.length ? chatList(sessions, project.id, true) : (
                    <p className="py-1.5 pl-8 text-xs text-claude-textSecondary">{t("sidebar.noProjectChats")}</p>
                  ))}
                </div>
              );
            })}
          </div>
        )}
      </section>
        <section
          aria-label={t("sidebar.recentChats")}
          className="mt-4"
          data-recents-mode={mode === 'code' ? 'code' : 'home'}
        >
          <button
            type="button"
            className="project-section-heading"
            aria-expanded={!recentsCollapsed}
            onClick={() => toggleRecents(recentsMode)}
          >
            {t("sidebar.recents")}
            <ChevronDown
              size={13}
              className={recentsCollapsed ? "-rotate-90" : ""}
            />
          </button>
          {!recentsCollapsed && (recent.length ? chatList(recent, "recents") : (
            <p className="px-2 py-1.5 text-xs text-claude-textSecondary">{t("sidebar.noRecentChats")}</p>
          ))}
        </section>
      {error && (
        <div
          role="alert"
          className="flex items-start gap-2 px-2 py-2 text-xs text-red-500"
        >
          <span className="min-w-0 flex-1 break-words">{error}</span>
          <button
            type="button"
            aria-label={t("sidebar.dismissError")}
            onClick={() => setError("")}
          >
            <X size={14} />
          </button>
        </div>
      )}
      {notice && (
        <p
          role="status"
          className="px-2 py-2 text-xs text-claude-textSecondary"
        >
          {notice}
        </p>
      )}
      {popup &&
        createPortal(
          <div
            ref={popupRef}
            data-project-popup
            className="fixed z-[190] text-claude-text"
            style={{
              left: popup.left,
              top: popup.top,
              width: popup.kind === "details" ? 310 : 260,
              maxWidth: "calc(100vw - 16px)",
            }}
            onMouseEnter={stopHover}
            onMouseLeave={() => {
              if (popup.kind === "details") {
                stopHover();
                hoverTimer.current = setTimeout(() => setPopup(null), 220);
              }
            }}
          >
            {popup.kind === "project" && popupProject && (
              <Menu
                items={projectMenu(popupProject)}
                close={() => setPopup(null)}
              />
            )}
            {popup.kind === "chat" && popupChat && (
              <Menu items={chatMenu(popupChat)} close={() => setPopup(null)} />
            )}
            {popup.kind === "details" && popupProject && (
              <div className="project-popup-menu p-3 text-[13px]">
                <div className="flex items-center gap-2">
                  <Folder size={15} />
                  <strong className="min-w-0 flex-1 truncate">
                    {popupProject.name}
                  </strong>
                  <button
                    type="button"
                    title={
                      preferences.pinnedProjects.includes(popupProject.id)
                        ? t("sidebar.unpinProject")
                        : t("sidebar.pinProject")
                    }
                    onClick={() =>
                      preferences.toggle("pinnedProjects", popupProject.id)
                    }
                  >
                    <Pin
                      size={14}
                      fill={
                        preferences.pinnedProjects.includes(popupProject.id)
                          ? "currentColor"
                          : "none"
                      }
                    />
                  </button>
                </div>
                <div className="my-2 flex items-center gap-2 text-claude-textSecondary">
                  <MessageCircle size={14} />
                  {
                    chats.filter(
                      (chat) =>
                        chat.project_id === popupProject.id && !chat.archived,
                    ).length
                  }{" "}
                  {t("sidebar.tasks")}
                </div>
                <div className="border-y border-claude-border py-2 max-h-28 overflow-auto">
                  {(popupProject.folders || []).map((folder) => (
                    <button
                      type="button"
                      key={folder.path}
                      onClick={() => void openFolder(folder.path)}
                      className="my-1 flex w-full items-start gap-2 text-left"
                    >
                      <Folder
                        size={14}
                        className="mt-0.5 shrink-0 text-claude-textSecondary"
                      />
                      <span className="break-all">{folder.path}</span>
                    </button>
                  ))}
                </div>
                <button
                  type="button"
                  className="mt-2 flex items-center gap-2"
                  onClick={() => {
                    setEditing(popupProject);
                    setPopup(null);
                  }}
                >
                  <Settings size={14} />
                  {t("sidebar.editProject")}
                </button>
              </div>
            )}
          </div>,
          document.body,
        )}
      {creating &&
        createPortal(
          <CreateProjectDialog
            onClose={() => setCreating(false)}
            onCreated={project => {
              setCreating(false);
              navigate("/", { state: { createdProjectId: project.id } });
            }}
          />,
          document.body,
        )}
      {editing &&
        createPortal(
          <EditProjectDialog
            key={editing.id}
            project={editing}
            onClose={() => setEditing(null)}
            onRemove={() => {
              setRemoving(editing);
              setEditing(null);
            }}
          />,
          document.body,
        )}
      {trust &&
        createPortal(
          <TrustProjectDialog
            project={trust.project}
            onClose={() => setTrust(null)}
            onTrusted={async () => {
              await trust.action();
              setTrust(null);
            }}
          />,
          document.body,
        )}
      {(removing || renaming) &&
        createPortal(
          <div
            className="fixed inset-0 z-[220] flex items-center justify-center bg-black/25 p-4"
            onKeyDown={(event) => {
              if (event.key === "Escape" && !busy) {
                setRemoving(null);
                setRenaming(null);
              }
            }}
          >
            <form
              ref={dialogRef}
              role="dialog"
              aria-modal="true"
              aria-labelledby="sidebar-dialog-title"
              className="w-[440px] max-w-full rounded-2xl border border-claude-border bg-claude-bg p-5 shadow-xl text-claude-text"
              onSubmit={(event) => {
                event.preventDefault();
                void perform(async () => {
                  if (removing) {
                    await deleteProject(removing.id);
                    if (
                      useWorkspaceModeStore.getState().lastCodeProjectId ===
                      removing.id
                    )
                      useWorkspaceModeStore
                        .getState()
                        .setLastCodeProjectId(null);
                    window.dispatchEvent(new Event("projectsChanged"));
                    window.dispatchEvent(new Event("conversationUpdated"));
                    setRemoving(null);
                  }
                  if (renaming && name.trim()) {
                    await patchChat(renaming, { title: name.trim() });
                    window.dispatchEvent(new Event("conversationTitleUpdated"));
                    setRenaming(null);
                  }
                });
              }}
            >
              <h2
                id="sidebar-dialog-title"
                className="mb-4 text-lg font-semibold"
              >
                {removing ? t("sidebar.removeNamedProject", { name: removing.name }) : t("sidebar.renameChat")}
              </h2>
              {removing ? (
                <p className="text-sm text-claude-textSecondary">
                  {t("sidebar.removeProjectDescription")}
                </p>
              ) : (
                <input
                  autoFocus
                  aria-label={t("sidebar.chatName")}
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  className="w-full rounded-lg border border-claude-border bg-claude-input px-3 py-2 outline-none focus:border-blue-500"
                />
              )}
              {error && (
                <p role="alert" className="mt-3 text-sm text-red-500">
                  {error}
                </p>
              )}
              <div className="mt-5 flex justify-end gap-3 text-sm">
                <button
                  autoFocus={!!removing}
                  type="button"
                  disabled={busy}
                  onClick={() => {
                    setRemoving(null);
                    setRenaming(null);
                  }}
                >
                  {t("common.cancel")}
                </button>
                <button
                  type="submit"
                  disabled={busy || (!!renaming && !name.trim())}
                  className="rounded-lg bg-claude-text px-4 py-2 text-claude-bg disabled:opacity-40"
                >
                  {busy ? t("sidebar.saving") : removing ? t("sidebar.removeProject") : t("common.save")}
                </button>
              </div>
            </form>
          </div>,
          document.body,
        )}
    </div>
  );
}
