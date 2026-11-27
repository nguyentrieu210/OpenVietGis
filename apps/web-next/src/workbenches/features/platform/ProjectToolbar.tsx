import "./platform.css";

interface ProjectToolbarProps {
  organizationId: string;
  projectId: string;
  active: "editing" | "queries" | "processing" | "cartography" | "hub";
  title: string;
}

export function ProjectToolbar({ title }: ProjectToolbarProps) {
  return (
    <div className="ovg-toolbar-compact" aria-label={title}>
      <h1>{title}</h1>
    </div>
  );
}
