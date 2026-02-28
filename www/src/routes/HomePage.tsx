import { Terminal, Copy, Check, Github, Cpu, GitBranch, Trello, Monitor, Activity, Beer, Package, BookOpen, Star } from "lucide-react"
import { useState, useEffect } from "react"
import { Link } from "react-router-dom"
import groveTui from "@/assets/grove_tui.webp"

const INSTALL_COMMANDS = [
  {
    id: "script",
    label: "Install Script",
    icon: Terminal,
    command: `curl -fsSL https://raw.githubusercontent.com/ZiiMs/Grove/main/install.sh | bash`,
  },
  {
    id: "homebrew",
    label: "Homebrew",
    icon: Beer,
    command: `brew install ZiiMs/grove/grove`,
  },
  {
    id: "cargo",
    label: "Cargo",
    icon: Package,
    command: `cargo install grove-ai`,
  },
]

const FEATURES = [
  {
    icon: Cpu,
    title: "Multi-Agent Management",
    description: "Run Claude Code, OpenCode, Gemini, and Codex simultaneously in isolated tmux sessions.",
  },
  {
    icon: GitBranch,
    title: "Git Worktree Isolation",
    description: "Each agent gets its own git branch and worktree—no more merge conflicts between tasks.",
  },
  {
    icon: Terminal,
    title: "GitHub / GitLab / Codeberg",
    description: "Create PRs/MRs, monitor pipelines, and merge directly from the terminal.",
  },
  {
    icon: Trello,
    title: "Project Management",
    description: "Connect to Asana, Linear, ClickUp, Notion, and Airtable for seamless task tracking.",
  },
  {
    icon: Monitor,
    title: "Dev Server Management",
    description: "Start, restart, and manage dev servers directly from the UI.",
  },
  {
    icon: Activity,
    title: "Live Metrics",
    description: "Monitor CPU, memory, and agent output in real-time with diff views.",
  },
]

function CopyButton({ command }: { command: string }) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    await navigator.clipboard.writeText(command)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <button
      onClick={copy}
      className="ml-3 p-1.5 rounded bg-[#1a1a1a] hover:bg-[#252525] transition-colors"
      title="Copy command"
    >
      {copied ? <Check className="w-4 h-4 text-green-400" /> : <Copy className="w-4 h-4 text-gray-400" />}
    </button>
  )
}

function formatStarCount(count: number): string {
  if (count >= 1000) {
    return (count / 1000).toFixed(1).replace(/\.0$/, "") + "k"
  }
  return count.toString()
}

function GitHubStarsButton() {
  const [stars, setStars] = useState<number | null>(null)

  useEffect(() => {
    fetch("https://api.github.com/repos/ZiiMs/Grove")
      .then((res) => res.json())
      .then((data) => {
        if (typeof data.stargazers_count === "number") {
          setStars(data.stargazers_count)
        }
      })
      .catch(() => {})
  }, [])

  return (
    <a
      href="https://github.com/ZiiMs/Grove"
      target="_blank"
      rel="noopener noreferrer"
      className="inline-flex items-center gap-2 px-4 py-2 bg-[#111] rounded-l-lg text-gray-300 hover:text-white transition-colors text-sm"
    >
      <Star className="w-4 h-4" />
      {stars !== null ? formatStarCount(stars) : "—"}
    </a>
  )
}

function InstallTab({ label, icon: Icon, active, onClick }: {
  label: string
  icon: React.ComponentType<{ className?: string }>
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-2 px-4 py-2 text-sm font-medium transition-colors ${
        active
          ? "text-green-400 border-b-2 border-green-400"
          : "text-gray-400 hover:text-gray-200"
      }`}
    >
      <Icon className="w-4 h-4" />
      {label}
    </button>
  )
}

function HeroSection() {
  const [activeTab, setActiveTab] = useState("script")

  const activeCommand = INSTALL_COMMANDS.find((c) => c.id === activeTab)?.command || ""

  return (
    <section className="relative py-20 px-6 overflow-hidden">
      <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_top,_#0f2917_0%,_transparent_50%)]" />
      
      <div className="relative max-w-4xl mx-auto text-center">
        <pre className="text-green-400 text-[10px] sm:text-xs leading-tight mb-6 font-normal" style={{
          textShadow: "0 0 10px #22c55e, 0 0 20px #22c55e, 0 0 40px #22c55e, 0 0 80px #16a34a",
        }}>
{`
 ██████╗ ██████╗  ██████╗ ██╗   ██╗███████╗
 ██╔════╝ ██╔══██╗██╔═══██╗██║   ██║██╔════╝
 ██║  ███╗██████╔╝██║   ██║██║   ██║█████╗  
 ██║   ██║██╔══██╗██║   ██║╚██╗ ██╔╝██╔══╝  
 ╚██████╔╝██║  ██║╚██████╔╝ ╚████╔╝ ███████╗
  ╚═════╝ ╚═╝  ╚═╝ ╚═════╝   ╚═══╝  ╚══════╝
`}
        </pre>
        
        <p className="text-xl text-green-400 mb-10 font-medium">
          AI Agent Workstation
        </p>

        <div className="flex flex-col items-center">
          <div className="flex gap-1 mb-0 bg-[#111] rounded-t-lg p-1">
            {INSTALL_COMMANDS.map((cmd) => (
              <InstallTab
                key={cmd.id}
                label={cmd.label}
                icon={cmd.icon}
                active={activeTab === cmd.id}
                onClick={() => setActiveTab(cmd.id)}
              />
            ))}
          </div>

          <div className="flex items-center bg-[#0f0f0f] border border-[#2a2a2a] rounded-b-lg px-4 py-3 max-w-xl w-full">
            <code className="text-green-400 text-sm flex-1 text-left overflow-x-auto">
              {activeCommand}
            </code>
            <CopyButton command={activeCommand} />
          </div>

          <div className="flex mt-6 border border-[#222] rounded-lg has-[:hover]:border-green-500/50 transition-colors">
            <GitHubStarsButton />
            <Link
              to="/docs"
              className="inline-flex items-center gap-2 px-4 py-2 bg-[#111] rounded-r-lg text-gray-300 hover:text-white transition-colors text-sm border-l border-[#222] hover:border-green-500/50"
            >
              <BookOpen className="w-4 h-4" />
              Read the docs
            </Link>
          </div>
        </div>
      </div>
    </section>
  )
}

function FeaturesSection() {
  return (
    <section className="py-20 px-6 bg-[#0d0d0d]">
      <div className="max-w-5xl mx-auto">
        <h2 className="text-2xl font-bold text-white text-center mb-4">
          Everything you need
        </h2>
        <p className="text-gray-400 text-center mb-12 max-w-xl mx-auto">
          Powerful features designed for developers who want to leverage multiple AI coding assistants.
        </p>

        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
          {FEATURES.map((feature) => (
            <div
              key={feature.title}
              className="bg-[#111] border border-[#222] rounded-lg p-5 hover:border-green-500/30 transition-colors group"
            >
              <feature.icon className="w-6 h-6 text-green-400 mb-3 group-hover:text-green-300 transition-colors" />
              <h3 className="text-white font-medium mb-2">{feature.title}</h3>
              <p className="text-gray-400 text-sm leading-relaxed">
                {feature.description}
              </p>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}

function ScreenshotsSection() {
  return (
    <section className="py-20 px-6">
      <div className="max-w-5xl mx-auto">
        <div className="bg-[#0c0c0c] border border-[#333] rounded-xl overflow-hidden shadow-2xl shadow-black/50">
          <div className="flex items-center gap-2 px-4 py-3 bg-[#1a1a1a] border-b border-[#333]">
            <div className="w-3 h-3 rounded-full bg-[#ff5f56]" />
            <div className="w-3 h-3 rounded-full bg-[#ffbd2e]" />
            <div className="w-3 h-3 rounded-full bg-[#27c93f]" />
            <span className="ml-4 text-gray-500 text-sm">grove — tmux</span>
          </div>
          <div className="p-2">
            <img 
              src={groveTui} 
              alt="Grove TUI Interface" 
              className="w-full rounded-lg"
            />
          </div>
        </div>
      </div>
    </section>
  )
}

function Footer() {
  return (
    <footer className="py-12 px-6 border-t border-[#1a1a1a]">
      <div className="max-w-4xl mx-auto flex flex-col sm:flex-row items-center justify-between gap-4">
        <div className="flex items-center gap-6">
          <div className="flex items-center gap-2 text-gray-400">
            <Github className="w-5 h-5" />
            <a
              href="https://github.com/ZiiMs/Grove"
              target="_blank"
              rel="noopener noreferrer"
              className="hover:text-white transition-colors"
            >
              ZiiMs/Grove
            </a>
          </div>
          <Link
            to="/docs"
            className="flex items-center gap-2 text-gray-400 hover:text-white transition-colors"
          >
            <BookOpen className="w-5 h-5" />
            Docs
          </Link>
        </div>

        <div className="text-gray-500 text-sm">
          MIT License
        </div>
      </div>
    </footer>
  )
}

function HomeComponent() {
  return (
    <div className="min-h-screen bg-[#0a0a0a]">
      <HeroSection />
      <FeaturesSection />
      <ScreenshotsSection />
      <Footer />
    </div>
  )
}

export default HomeComponent
