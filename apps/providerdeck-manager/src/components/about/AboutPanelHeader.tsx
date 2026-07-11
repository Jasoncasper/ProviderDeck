interface AboutPanelHeaderProps {
  title: string;
  description: string;
}

export function AboutPanelHeader({ title, description }: AboutPanelHeaderProps) {
  return (
    <div>
      <h3 className="font-semibold">{title}</h3>
      <p className="text-xs text-muted-foreground">{description}</p>
    </div>
  );
}
