export function UnavailablePage({ title, description }: { title: string; description: string }) {
  return <><div className="page-head"><div><h1>{title}</h1><div className="page-sub">Edith parity work in progress</div></div></div><section className="settings-info"><div className="info-glyph">◇</div><h2>{title} is tracked, not simulated</h2><p>{description}</p><span className="pill warn">Not implemented yet</span></section></>;
}
