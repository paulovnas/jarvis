import type { ReactElement } from "react";
import { FolderInput, Pencil, Trash2 } from "lucide-react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";

export function LibraryItemMenu({
  children,
  disabled,
  onEdit,
  onDelete,
  onMove,
}: {
  children: ReactElement;
  disabled: boolean;
  onEdit: () => void;
  onDelete: () => void;
  onMove?: () => void;
}) {
  if (disabled) return children;
  return (
    <ContextMenu>
      <ContextMenuTrigger render={children} />
      <ContextMenuContent>
        <ContextMenuGroup>
          <ContextMenuItem
            className="cursor-pointer"
            disabled={disabled}
            onClick={onEdit}
          >
            <Pencil />
            Editar
          </ContextMenuItem>
          <ContextMenuSeparator />
          {onMove && <ContextMenuItem className="cursor-pointer" onClick={onMove}><FolderInput />Mover para outro workspace</ContextMenuItem>}
          <ContextMenuItem variant="destructive" className="cursor-pointer" disabled={disabled} onClick={onDelete}>
            <Trash2 />
            Excluir
          </ContextMenuItem>
        </ContextMenuGroup>
      </ContextMenuContent>
    </ContextMenu>
  );
}
