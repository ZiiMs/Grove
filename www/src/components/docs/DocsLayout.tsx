import { useState, useEffect } from "react"
import { Menu, X, Home } from "lucide-react"
import { Link } from "react-router-dom"

interface NavItem {
  id: string
  label: string
  children?: NavItem[]
}

const NAV_ITEMS: NavItem[] = [
  { id: "installation", label: "Installation" },
  { id: "features", label: "Features" },
  { id: "keybinds", label: "Keybinds", children: [
    { id: "keybinds-navigation", label: "Navigation" },
    { id: "keybinds-agent", label: "Agent Management" },
    { id: "keybinds-git", label: "Git Operations" },
    { id: "keybinds-view", label: "View Controls" },
    { id: "keybinds-external", label: "External Services" },
    { id: "keybinds-pm", label: "Project Management" },
    { id: "keybinds-devserver", label: "Dev Server" },
    { id: "keybinds-other", label: "Other" },
  ]},
  { id: "configuration", label: "Configuration", children: [
    { id: "config-global", label: "Global Config" },
    { id: "config-repo", label: "Repo Config" },
    { id: "config-env", label: "Environment Variables" },
  ]},
  { id: "integrations", label: "Integrations", children: [
    { id: "integrations-git", label: "Git Providers" },
    { id: "integrations-pm", label: "Project Management" },
  ]},
  { id: "architecture", label: "Architecture" },
]

function SidebarNav({ items, activeId, onItemClick, mobile = false }: {
  items: NavItem[]
  activeId: string
  onItemClick: (id: string) => void
  mobile?: boolean
}) {
  return (
    <nav className="space-y-1">
      {items.map((item) => (
        <div key={item.id}>
          <a
            href={`#${item.id}`}
            onClick={(e) => {
              e.preventDefault()
              onItemClick(item.id)
            }}
            className={`block px-3 py-2 text-sm rounded transition-colors ${
              activeId === item.id
                ? "text-green-400 bg-green-400/10"
                : "text-gray-400 hover:text-gray-200 hover:bg-white/5"
            } ${item.children ? "font-medium" : ""} ${mobile ? "py-3" : ""}`}
          >
            {item.label}
          </a>
          {item.children && (
            <div className="ml-4 mt-1 space-y-0.5 border-l border-[#222] pl-2">
              {item.children.map((child) => (
                <a
                  key={child.id}
                  href={`#${child.id}`}
                  onClick={(e) => {
                    e.preventDefault()
                    onItemClick(child.id)
                  }}
                  className={`block px-3 py-1.5 text-xs rounded transition-colors ${
                    activeId === child.id
                      ? "text-green-400 bg-green-400/10"
                      : "text-gray-500 hover:text-gray-300 hover:bg-white/5"
                  }`}
                >
                  {child.label}
                </a>
              ))}
            </div>
          )}
        </div>
      ))}
    </nav>
  )
}

export function DocsLayout({ children }: { children: React.ReactNode }) {
  const [activeId, setActiveId] = useState("installation")
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)

  const allIds = NAV_ITEMS.flatMap((item) => [
    item.id,
    ...(item.children?.map((c) => c.id) || []),
  ])

  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.filter((e) => e.isIntersecting)
        if (visible.length > 0) {
          const sorted = visible.sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)
          setActiveId(sorted[0].target.id)
        }
      },
      { rootMargin: "-20% 0px -70% 0px" }
    )

    allIds.forEach((id) => {
      const el = document.getElementById(id)
      if (el) observer.observe(el)
    })

    return () => observer.disconnect()
  }, [allIds.join(",")])

  const scrollToSection = (id: string) => {
    const el = document.getElementById(id)
    if (el) {
      el.scrollIntoView({ behavior: "smooth" })
      setActiveId(id)
      setMobileMenuOpen(false)
    }
  }

  return (
    <div className="min-h-screen bg-[#0a0a0a]">
      <header className="sticky top-0 z-50 bg-[#0a0a0a]/95 backdrop-blur border-b border-[#1a1a1a]">
        <div className="max-w-6xl mx-auto px-4 h-14 flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Link to="/" className="text-gray-400 hover:text-white transition-colors">
              <Home className="w-5 h-5" />
            </Link>
            <span className="text-white font-medium">Grove Docs</span>
          </div>
          <button
            onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
            className="md:hidden p-2 text-gray-400 hover:text-white"
          >
            {mobileMenuOpen ? <X className="w-5 h-5" /> : <Menu className="w-5 h-5" />}
          </button>
        </div>
      </header>

      <div className="max-w-6xl mx-auto flex">
        <aside className="hidden md:block w-56 shrink-0">
          <div className="sticky top-14 h-[calc(100vh-3.5rem)] overflow-y-auto py-6 px-4">
            <SidebarNav
              items={NAV_ITEMS}
              activeId={activeId}
              onItemClick={scrollToSection}
            />
          </div>
        </aside>

        {mobileMenuOpen && (
          <div className="md:hidden fixed inset-0 top-14 z-40 bg-[#0a0a0a] border-t border-[#1a1a1a] overflow-y-auto">
            <div className="p-4">
              <SidebarNav
                items={NAV_ITEMS}
                activeId={activeId}
                onItemClick={scrollToSection}
                mobile
              />
            </div>
          </div>
        )}

        <main className="flex-1 min-w-0 px-4 md:px-8 py-8">
          {children}
        </main>
      </div>
    </div>
  )
}
