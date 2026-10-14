import { useState } from "react";
import { useI18n } from "../../hooks/useI18n";
import { Folder, X } from "lucide-react";
import { Project, updateProject } from "../../api";
import FolderFields from "./FolderFields";
import { useDialogFocus } from "./useDialogFocus";

export default function EditProjectDialog({
  project,
  onClose,
  onRemove,
}: {
  project: Project;
  onClose: () => void;
  onRemove: () => void;
}) {
  const { t } = useI18n();
  const [name, setName] = useState(project.name);
  const [folders, setFolders] = useState(
    project.folders?.map((folder) => folder.path) || [],
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const dialogRef = useDialogFocus(true);
  const save = async () => {
    if (busy || !name.trim() || !folders.length) return;
    setBusy(true);
    try {
      await updateProject(project.id, { name: name.trim(), folders });
      window.dispatchEvent(new Event("projectsChanged"));
      window.dispatchEvent(new Event("conversationUpdated"));
      onClose();
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div
      className="fixed inset-0 z-[220] flex items-center justify-center bg-black/25 p-4"
      onKeyDown={(event) => {
        if (event.key === "Escape" && !busy) onClose();
      }}
    >
      <form
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="edit-project-title"
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        className="w-[500px] max-w-full max-h-[85vh] overflow-y-auto rounded-2xl border border-claude-border bg-claude-bg p-4 shadow-xl text-claude-text"
      >
        <div className="mb-3 flex items-center justify-between">
          <h2 id="edit-project-title" className="text-lg font-semibold">
            {t("sidebar.editProject")}
          </h2>
          <button
            type="button"
            disabled={busy}
            aria-label={t("projectDialogs.closeEdit")}
            onClick={onClose}
          >
            <X size={16} />
          </button>
        </div>
        <label className="mb-3 flex items-center gap-3 rounded-xl border border-claude-border bg-claude-input px-3 focus-within:border-blue-500">
          <Folder size={16} />
          <input
            autoFocus
            aria-label={t("projectDialogs.name")}
            value={name}
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
            className="w-full bg-transparent py-2.5 text-sm outline-none"
          />
        </label>
        <FolderFields
          folders={folders}
          onChange={setFolders}
          disabled={busy}
          compact
        />
        {error && (
          <p role="alert" className="mt-3 text-sm text-red-500">
            {error}
          </p>
        )}
        <div className="mt-5 flex items-center gap-3 text-sm">
          <button
            type="button"
            disabled={busy}
            onClick={onRemove}
            className="mr-auto rounded-lg bg-red-500/10 px-3 py-2 text-red-500"
          >
            {t("projectDialogs.removeLocal")}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={onClose}
            className="text-claude-textSecondary"
          >
            {t("common.cancel")}
          </button>
          <button
            type="submit"
            disabled={busy || !name.trim() || !folders.length}
            className="rounded-lg bg-claude-text px-4 py-2 text-claude-bg disabled:opacity-40"
          >
            {busy ? t("sidebar.saving") : t("common.save")}
          </button>
        </div>
      </form>
    </div>
  );
}
