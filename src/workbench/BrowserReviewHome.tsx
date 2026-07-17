import { WindowBrand } from "./WindowChrome";

const REVIEW_SURFACES = [
  {
    href: "/dashboard-review.html",
    title: "Dashboard",
    description: "Repositorios, filtros, carga y adaptación del panel principal.",
  },
  {
    href: "/agent-lens-restorable.html",
    title: "Agents y Agent Lens",
    description: "Conversaciones completadas, activas, archivadas y su inspector.",
  },
  {
    href: "/agent-runtime.html",
    title: "Runtime de Agent",
    description: "Composer y selección de configuraciones guardadas sin lanzar procesos reales.",
  },
  {
    href: "/demo.html",
    title: "Live Diff ruler",
    description: "Navegación por cambios y señales densas del archivo.",
  },
] as const;

export function BrowserReviewHome() {
  return (
    <main className="browser-review" data-testid="browser-review-home">
      <header className="browser-review__head">
        <WindowBrand />
        <span>Modo de revisión web</span>
      </header>
      <section className="browser-review__intro" aria-labelledby="browser-review-title">
        <p className="browser-review__eyebrow">QA visual y responsive</p>
        <h1 id="browser-review-title">Elige una superficie para revisar</h1>
        <p>
          Este modo no ejecuta comandos Rust ni procesos de Agent. Para probar IPC, archivos o
          sesiones reales, abre la aplicación de escritorio o el E2E nativo aislado.
        </p>
      </section>
      <nav className="browser-review__surfaces" aria-label="Superficies de revisión disponibles">
        {REVIEW_SURFACES.map((surface) => (
          <a href={surface.href} key={surface.href}>
            <strong>{surface.title}</strong>
            <span>{surface.description}</span>
          </a>
        ))}
      </nav>
    </main>
  );
}
