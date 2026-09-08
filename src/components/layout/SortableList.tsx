import { type CSSProperties, type ReactNode } from "react";
import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, useSortable, sortableKeyboardCoordinates, verticalListSortingStrategy, horizontalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { moveItem } from "@/core/item-order";

export function SortableList({ ids, horizontal = false, onReorder, onSortingChange, children }: { ids: string[]; horizontal?: boolean; onReorder: (ids: string[]) => void; onSortingChange?: (sorting: boolean) => void; children: ReactNode }) {
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }));
  return <DndContext sensors={sensors} collisionDetection={closestCenter} onDragStart={() => onSortingChange?.(true)} onDragCancel={() => onSortingChange?.(false)} accessibility={{ screenReaderInstructions: { draggable: "Pressione espaço para reordenar, use as setas e pressione espaço para confirmar ou Escape para cancelar." }, announcements: {
    onDragStart: () => "Reordenação iniciada.", onDragOver: () => "Posição de destino alterada.", onDragEnd: () => "Reordenação concluída.", onDragCancel: () => "Reordenação cancelada.",
  } }} onDragEnd={({ active, over }) => { onSortingChange?.(false); if (over && active.id !== over.id) onReorder(moveItem(ids, String(active.id), String(over.id))); }}>
    <SortableContext items={ids} strategy={horizontal ? horizontalListSortingStrategy : verticalListSortingStrategy}>{children}</SortableContext>
  </DndContext>;
}

export function SortableItem({ id, disabled, children }: { id: string; disabled?: boolean; children: (props: ReturnType<typeof useSortable> & { style: CSSProperties }) => ReactNode }) {
  const sortable = useSortable({ id, disabled });
  return children({ ...sortable, style: { transform: CSS.Transform.toString(sortable.transform), transition: sortable.transition, opacity: sortable.isDragging ? 0.65 : undefined, position: "relative", zIndex: sortable.isDragging ? 10 : undefined } });
}
