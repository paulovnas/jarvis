ALTER TABLE projects ADD COLUMN icon TEXT NOT NULL DEFAULT 'folder'
  CHECK (icon IN ('bot','workflow','route','brain','search','code','palette','shield','terminal','wrench','book','sparkles','target','pen','lightbulb','rocket','folder','folder-code','package','database','globe','app-window'));
--> statement-breakpoint
ALTER TABLE projects ADD COLUMN color TEXT NOT NULL DEFAULT 'cyan'
  CHECK (color IN ('blue','green','cyan','yellow','red','purple','neutral'));
