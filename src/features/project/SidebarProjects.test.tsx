import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import SidebarProjects from "./SidebarProjects";
import { SidebarChat, useSidebarPreferences } from "./sidebarModel";
import { useWorkspaceModeStore } from "../../stores/useWorkspaceModeStore";
import {
  deleteProject,
  getConversation,
  getProjects,
  updateConversation,
  updateProject,
} from "../../api";

vi.mock("../../api", () => ({
  getProjects: vi.fn(),
  updateConversation: vi.fn(),
  updateProject: vi.fn(),
  deleteProject: vi.fn(),
  getConversation: vi.fn(),
  createProjectConversation: vi.fn(),
  createProject: vi.fn(),
  trustProjectFolders: vi.fn(),
}));
vi.mock("../../utils/tauriAPI", () => ({
  tauriAPI: { isTauri: false, openFolder: vi.fn() },
}));
const project = {
  id: "p1",
  name: "Desktop",
  workspace_path: "C:\\work\\desktop",
  folders: [{ path: "C:\\work\\desktop", trusted: true }],
  description: "",
  instructions: "",
  created_at: "",
  updated_at: "",
  is_archived: 0,
};
const other = { ...project, id: "p2", name: "Browser" };
const chats: SidebarChat[] = [
  {
    id: "a",
    title: "Pinned task",
    project_id: "p1",
    workspace_mode: "code",
    pinned: true,
  },
  { id: "b", title: "Project task", project_id: "p1", workspace_mode: "code" },
  { id: "c", title: "Loose task", workspace_mode: "code" },
  {
    id: "d",
    title: "Archived task",
    project_id: "p1",
    workspace_mode: "code",
    archived: true,
  },
  {
    id: "e",
    title: "Chat mode task",
    project_id: "p1",
    workspace_mode: "chat",
  },
];
beforeEach(() => {
  vi.resetAllMocks();
  localStorage.clear();
  useWorkspaceModeStore.setState({
    mode: "code",
    recentsCollapsed: { chat: false, work: false, study: false, code: false },
  });
  useSidebarPreferences.setState({
    pinnedProjects: [],
    unreadChats: [],
    collapsedProjects: [],
    pinnedCollapsed: false,
    projectsCollapsed: false,
  });
  vi.mocked(getProjects).mockResolvedValue([project, other]);
  vi.mocked(updateConversation).mockResolvedValue({});
});
const setup = async (initial = chats) => {
  const open = vi.fn();
  const result = render(
    <MemoryRouter>
      <SidebarProjects
        chats={initial}
        streamingIds={new Set()}
        onOpenChat={open}
      />
    </MemoryRouter>,
  );
  await screen.findByRole("button", { name: "Desktop" });
  return { ...result, open };
};
const chatMenu = (title = "Project task") =>
  fireEvent.click(
    within(screen.getByRole("region", { name: "Projects" })).getByRole("button", { name: `Chat options for ${title}` }),
  );
const projectMenu = () =>
  fireEvent.click(
    screen.getByRole("button", { name: "Project options for Desktop" }),
  );

describe("project sidebar", () => {
  it.each(["chat", "work", "study", "code"] as const)("keeps empty Recents visible in %s", async mode => {
    useWorkspaceModeStore.setState({ mode });
    await setup([]);
    const recents = screen.getByRole("region", { name: "Recent chats" });
    expect(within(recents).getByRole("button", { name: "Recents" })).toHaveAttribute("aria-expanded", "true");
    expect(within(recents).getByText("No recent chats")).toBeInTheDocument();
    fireEvent.click(within(recents).getByRole("button", { name: "Recents" }));
    expect(within(recents).queryByText("No recent chats")).not.toBeInTheDocument();
    expect(within(recents).getByRole("button", { name: "Recents" })).toBeInTheDocument();
  });
  it.each(["chat", "work", "study", "code"] as const)("includes project sessions in %s Recents without mixing Home and Code", async mode => {
    useWorkspaceModeStore.setState({ mode });
    await setup([
      { id: "own", title: "Current project task", project_id: "p1", workspace_mode: mode },
      { id: "other-mode", title: "Other mode task", project_id: "p1", workspace_mode: mode === "code" ? "chat" : "code" },
      { id: "archived", title: "Archived", project_id: "p1", workspace_mode: mode, archived: true },
    ]);
    const recents = screen.getByRole("region", { name: "Recent chats" });
    expect(within(recents).getByText("Current project task")).toBeInTheDocument();
    expect(within(recents).queryByText("Other mode task")).not.toBeInTheDocument();
    expect(screen.queryByText("Archived")).not.toBeInTheDocument();
    expect(within(recents).getByText("Current project task").closest(".project-sidebar-row")).not.toHaveClass("project-session-row");
  });
  it.each(["chat", "work", "study"] as const)("combines Home conversations and pins while composing in %s", async mode => {
    useWorkspaceModeStore.setState({ mode });
    await setup([
      { id: "chat", title: "Chat session", workspace_mode: "chat" },
      { id: "work", title: "Work session", workspace_mode: "work", project_id: "p1" },
      { id: "study", title: "Study session", workspace_mode: "study", project_id: "p1" },
      { id: "legacy", title: "Legacy cowork session", workspace_mode: "cowork" },
      { id: "pin", title: "Study pin", workspace_mode: "study", pinned: true },
      { id: "code", title: "Code session", workspace_mode: "code" },
      { id: "code-pin", title: "Code pin", workspace_mode: "code", pinned: true },
    ]);
    const recents = screen.getByRole("region", { name: "Recent chats" });
    for (const title of ["Chat session", "Work session", "Study session", "Legacy cowork session"]) {
      expect(within(recents).getByText(title)).toBeInTheDocument();
    }
    expect(within(recents).queryByText("Code session")).not.toBeInTheDocument();
    const pinned = screen.getByRole("region", { name: "Pinned chats" });
    expect(within(pinned).getByText("Study pin")).toBeInTheDocument();
    expect(within(pinned).queryByText("Code pin")).not.toBeInTheDocument();
    fireEvent.click(within(recents).getByRole("button", { name: "Recents" }));
    expect(useWorkspaceModeStore.getState().recentsCollapsed.chat).toBe(true);
    expect(useWorkspaceModeStore.getState().recentsCollapsed.code).toBe(false);
  });
  it("expands a Code project's sessions in Chat, including pinned sessions, and opens the selected chat", async () => {
    useWorkspaceModeStore.setState({ mode: "chat" });
    const { open } = await setup(chats.filter(chat => chat.workspace_mode === "code"));
    const projects = screen.getByRole("region", { name: "Projects" });
    const projectButton = within(projects).getByRole("button", { name: "Desktop" });
    fireEvent.click(projectButton);
    expect(within(projects).queryByText("Project task")).not.toBeInTheDocument();
    fireEvent.click(projectButton);
    expect(within(projects).getByText("Project task")).toBeInTheDocument();
    expect(within(projects).getByText("Pinned task")).toBeInTheDocument();
    fireEvent.click(within(projects).getByText("Project task"));
    expect(open).toHaveBeenCalledWith("b");
    expect(useWorkspaceModeStore.getState().mode).toBe("chat");
  });
  it("shows an empty project message only while that project is expanded", async () => {
    await setup([]);
    const projectButton = screen.getByRole("button", { name: "Desktop" });
    const projectGroup = projectButton.closest("[data-project-id]")!.parentElement!;
    expect(within(projectGroup).getByText("No chats in this project")).toBeInTheDocument();
    fireEvent.click(projectButton);
    expect(within(projectGroup).queryByText("No chats in this project")).not.toBeInTheDocument();
  });
  it("places All projects in the heading and leaves recent sessions unindented", async () => {
    await setup();
    const projects = screen.getByRole("region", { name: "Projects" });
    const heading = within(projects).getByRole("button", { name: "Projects" });
    expect(heading.parentElement).toContainElement(screen.getByRole("button", { name: "All projects" }));
    expect(screen.getByText("Loose task").closest(".project-sidebar-row")).not.toHaveClass("project-session-row");
    expect(within(screen.getByRole("region", { name: "Projects" })).getByText("Project task").closest(".project-sidebar-row")).toHaveClass("project-session-row");
  });
  it("executes the pin shortcut for the selected menu chat and ignores it while editing", async () => {
    await setup();
    chatMenu();
    fireEvent.keyDown(window, { key: "p", ctrlKey: true, altKey: true });
    await waitFor(() => expect(updateConversation).toHaveBeenCalledWith("b", { pinned: true }));
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    chatMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: /^Rename/ }));
    fireEvent.keyDown(screen.getByLabelText("Chat name"), { key: "p", ctrlKey: true, altKey: true });
    expect(updateConversation).toHaveBeenCalledTimes(1);
  });
  it("shows every mode in projects while keeping Recents mode-specific and collapses independently", async () => {
    await setup();
    expect(
      within(screen.getByRole("region", { name: "Pinned chats" })).getByText(
        "Pinned task",
      ),
    ).toBeInTheDocument();
    const projects = screen.getByRole("region", { name: "Projects" });
    expect(within(projects).getByText("Pinned task")).toBeInTheDocument();
    expect(within(projects).getByText("Project task")).toBeInTheDocument();
    expect(screen.queryByText("Archived task")).not.toBeInTheDocument();
    expect(within(projects).getByText("Chat mode task")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Recent chats" })).queryByText("Chat mode task")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Desktop" }));
    expect(within(projects).queryByText("Project task")).not.toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Recent chats" })).getByText("Project task")).toBeInTheDocument();
    expect(screen.getByText("Pinned task")).toBeInTheDocument();
    expect(screen.getByText("Loose task")).toBeInTheDocument();
  });
  it("saves pin changes through the API and reports failure without hiding the chat", async () => {
    await setup();
    chatMenu();
    vi.mocked(updateConversation).mockRejectedValueOnce(
      new Error("Could not save pin"),
    );
    fireEvent.click(screen.getByRole("menuitem", { name: /^Pin / }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not save pin",
    );
    expect(updateConversation).toHaveBeenCalledWith("b", { pinned: true });
    expect(within(screen.getByRole("region", { name: "Projects" })).getByText("Project task")).toBeInTheDocument();
  });
  it("moves a chat and can detach it without deleting it", async () => {
    await setup();
    chatMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Project" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Browser" }));
    await waitFor(() =>
      expect(updateConversation).toHaveBeenCalledWith("b", {
        project_id: "p2",
      }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
    );
    chatMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Project" }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: "Remove from Desktop" }),
    );
    await waitFor(() =>
      expect(updateConversation).toHaveBeenCalledWith("b", {
        project_id: null,
      }),
    );
  });
  it("requires folder trust before moving to an untrusted project", async () => {
    vi.mocked(getProjects).mockResolvedValue([
      project,
      { ...other, folders: [{ path: "C:\\work\\browser", trusted: false }] },
    ]);
    await setup();
    chatMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Project" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Browser" }));
    expect(
      await screen.findByRole("dialog", { name: "Trust this folder?" }),
    ).toBeInTheDocument();
    expect(updateConversation).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(updateConversation).not.toHaveBeenCalled();
  });
  it("edits the name and source folders without saving draft changes on cancel", async () => {
    await setup();
    projectMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Project name"), {
      target: { value: "Renamed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(updateProject).not.toHaveBeenCalled();
    projectMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Project name"), {
      target: { value: "Renamed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add folder" }));
    fireEvent.change(screen.getByLabelText("Folder path"), {
      target: { value: "C:\\work\\second" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(updateProject).toHaveBeenCalledWith("p1", {
        name: "Renamed",
        folders: [project.workspace_path, "C:\\work\\second"],
      }),
    );
  });
  it("archives all project chats including pinned chats but never deletes messages", async () => {
    await setup();
    projectMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Archive chats" }));
    await waitFor(() => expect(updateConversation).toHaveBeenCalledTimes(3));
    expect(updateConversation).toHaveBeenCalledWith("a", { archived: true });
    expect(updateConversation).toHaveBeenCalledWith("b", { archived: true });
    expect(updateConversation).toHaveBeenCalledWith("e", { archived: true });
  });
  it("requires confirmation to remove a project and preserves cancellation", async () => {
    await setup();
    projectMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove project" }));
    expect(
      screen.getByText(/files on disk are not deleted/),
    ).toBeInTheDocument();
    expect(deleteProject).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(deleteProject).not.toHaveBeenCalled();
  });
  it("persists unread state and clears it when opening that chat", async () => {
    const { open } = await setup();
    chatMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: /^Mark as unread/ }));
    expect(useSidebarPreferences.getState().unreadChats).toContain("b");
    expect(
      JSON.parse(localStorage.getItem("project-sidebar-preferences")!).state
        .unreadChats,
    ).toContain("b");
    fireEvent.click(within(screen.getByRole("region", { name: "Projects" })).getByText("Project task"));
    expect(open).toHaveBeenCalledWith("b");
    expect(useSidebarPreferences.getState().unreadChats).not.toContain("b");
  });
  it("copies complete Markdown content, including structured message text", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    vi.mocked(getConversation).mockResolvedValue({
      messages: [
        {
          role: "assistant",
          content: '[{"type":"text","text":"Full answer"}]',
        },
      ],
    } as never);
    await setup();
    chatMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Markdown" }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        "# Project task\n\n## assistant\n\nFull answer",
      ),
    );
  });
});
