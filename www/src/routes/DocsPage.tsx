import { Terminal, Copy, Check, Cpu, GitBranch, Monitor, Activity, Trello, Beer, Package } from "lucide-react"
import { useState } from "react"
import { DocsLayout } from "@/components/docs/DocsLayout"

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
    description: "Run Claude Code, OpenCode, Gemini, and Codex simultaneously in isolated tmux sessions. Each agent operates independently with its own terminal session.",
  },
  {
    icon: GitBranch,
    title: "Git Worktree Isolation",
    description: "Each agent gets its own git branch and worktree—no more merge conflicts between tasks. Worktrees can be stored in-project or centralized in ~/.grove/worktrees/.",
  },
  {
    icon: Terminal,
    title: "Git Provider Integration",
    description: "Full support for GitLab, GitHub, and Codeberg. Create PRs/MRs, monitor CI/CD pipelines, view statuses, and merge directly from the terminal.",
  },
  {
    icon: Trello,
    title: "Project Management",
    description: "Connect to Asana, Notion, ClickUp, Airtable, or Linear for seamless task tracking. Auto-link tasks to agents and update statuses automatically.",
  },
  {
    icon: Monitor,
    title: "Dev Server Management",
    description: "Start, restart, and manage dev servers directly from the UI. Configure auto-start, port detection, and symlinks for worktrees.",
  },
  {
    icon: Activity,
    title: "Live Metrics & Monitoring",
    description: "Monitor CPU, memory, and agent output in real-time with diff views. Detect agent status (running, waiting, error) automatically.",
  },
]

const KEYBINDS = {
  navigation: [
    { key: "↓ / j", action: "Move to next agent" },
    { key: "↑ / k", action: "Move to previous agent" },
    { key: "g", action: "Go to first agent" },
    { key: "G", action: "Go to last agent" },
    { key: "Tab", action: "Switch preview tab" },
  ],
  agent: [
    { key: "n", action: "Create new agent (prompts for branch name)" },
    { key: "d", action: "Delete selected agent" },
    { key: "Enter", action: "Attach to agent's tmux session" },
    { key: "N", action: "Set/edit custom note for agent" },
    { key: "s", action: "Request work summary (for Slack)" },
    { key: "y", action: "Copy agent/branch name to clipboard" },
  ],
  git: [
    { key: "c", action: "Copy cd command to worktree" },
    { key: "m", action: "Merge main into current branch" },
    { key: "p", action: "Push changes to remote" },
    { key: "f", action: "Fetch remote" },
  ],
  view: [
    { key: "/", action: "Toggle diff view" },
    { key: "L", action: "Toggle logs panel" },
    { key: "S", action: "Open settings" },
    { key: "C", action: "Toggle column visibility" },
  ],
  external: [
    { key: "o", action: "Open MR/PR in browser" },
    { key: "e", action: "Open worktree in editor" },
  ],
  pm: [
    { key: "a", action: "Assign task by URL/ID" },
    { key: "A", action: "Open task in browser" },
    { key: "t", action: "Browse tasks from project" },
    { key: "T", action: "Change linked task status" },
    { key: "f", action: "Filter tasks (in task list)" },
  ],
  devserver: [
    { key: "Ctrl+s", action: "Start dev server" },
    { key: "Ctrl+S", action: "Restart dev server" },
    { key: "C", action: "Clear logs" },
    { key: "O", action: "Open in browser" },
  ],
  other: [
    { key: "R", action: "Refresh all status" },
    { key: "?", action: "Toggle help overlay" },
    { key: "i", action: "Debug status (when enabled)" },
    { key: "q", action: "Quit" },
    { key: "Esc", action: "Cancel/close dialogs" },
    { key: "Ctrl+c", action: "Force quit" },
  ],
}

const GLOBAL_CONFIG = `[global]
ai_agent = "claude-code"  # claude-code, opencode, codex, gemini
log_level = "info"
worktree_location = "project"  # project or home
editor = "code {path}"  # Editor command template
debug_mode = false

[ui]
frame_rate = 30
tick_rate_ms = 250
output_buffer_lines = 5000
show_preview = true
show_metrics = true
show_logs = true
show_banner = true

[performance]
agent_poll_ms = 500
git_refresh_secs = 30
gitlab_refresh_secs = 60
github_refresh_secs = 60
codeberg_refresh_secs = 60`

const REPO_CONFIG = `[git]
provider = "gitlab"           # gitlab, github, codeberg
branch_prefix = "feature/"
main_branch = "main"
worktree_symlinks = ["node_modules", ".env"]

[git.gitlab]
project_id = 12345
base_url = "https://gitlab.com"

[git.github]
owner = "myorg"
repo = "myrepo"

[git.codeberg]
owner = "myorg"
repo = "myrepo"
base_url = "https://codeberg.org"
ci_provider = "forgejo-actions"  # forgejo-actions or woodpecker

[project_mgmt]
provider = "asana"  # asana, notion, clickup, airtable, linear

[project_mgmt.asana]
project_gid = "1201234567890"
in_progress_section_gid = "1201234567891"
done_section_gid = "1201234567892"

[dev_server]
command = "npm run dev"
port = 3000
auto_start = false
worktree_symlinks = ["node_modules", ".env"]

[automation]
on_task_assign = "Please work on this task: {task_url}"
on_push = "Review and commit these changes"
on_delete = "Clean up and finalize"`

const ENV_VARS = [
  { name: "GITLAB_TOKEN", description: "GitLab personal access token (api scope)" },
  { name: "GITHUB_TOKEN", description: "GitHub personal access token" },
  { name: "CODEBERG_TOKEN", description: "Codeberg access token" },
  { name: "ASANA_TOKEN", description: "Asana personal access token" },
  { name: "NOTION_TOKEN", description: "Notion integration token" },
  { name: "CLICKUP_TOKEN", description: "ClickUp API token" },
  { name: "AIRTABLE_TOKEN", description: "Airtable personal access token" },
  { name: "LINEAR_TOKEN", description: "Linear API token" },
  { name: "WOODPECKER_TOKEN", description: "Woodpecker CI token (for Codeberg)" },
]

const GIT_PROVIDERS = [
  {
    name: "GitLab",
    features: ["Merge requests", "Pipeline status", "Auto-detect MR creation", "Open in browser"],
    config: `project_id = 12345
base_url = "https://gitlab.com"`,
  },
  {
    name: "GitHub",
    features: ["Pull requests", "Actions status", "Auto-detect PR creation", "Open in browser"],
    config: `owner = "myorg"
repo = "myrepo"`,
  },
  {
    name: "Codeberg",
    features: ["Pull requests", "Forgejo Actions / Woodpecker CI", "Open in browser"],
    config: `owner = "myorg"
repo = "myrepo"
ci_provider = "forgejo-actions"`,
  },
]

const PM_PROVIDERS = [
  { name: "Asana", features: ["Task linking", "Status updates", "Section movement"] },
  { name: "Notion", features: ["Database integration", "Status property updates"] },
  { name: "ClickUp", features: ["List tasks", "Status updates"] },
  { name: "Airtable", features: ["Table integration", "Status field updates"] },
  { name: "Linear", features: ["Team tasks", "State transitions"] },
]

const ARCHITECTURE = [
  { path: "src/main.rs", desc: "Entry point, event loop, action processing" },
  { path: "src/lib.rs", desc: "Module exports" },
  { path: "src/agent/", desc: "Agent model, status detection, lifecycle" },
  { path: "src/app/", desc: "Application state, config, actions enum" },
  { path: "src/git/", desc: "Git operations, worktree management" },
  { path: "src/gitlab/", desc: "GitLab API client and types" },
  { path: "src/asana/", desc: "Asana API client and types" },
  { path: "src/storage/", desc: "Session persistence (JSON)" },
  { path: "src/tmux/", desc: "tmux session management" },
  { path: "src/ui/", desc: "TUI components (ratatui)" },
]

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <button
      onClick={copy}
      className="absolute top-3 right-3 p-1.5 rounded bg-[#1a1a1a] hover:bg-[#252525] transition-colors"
      title="Copy"
    >
      {copied ? <Check className="w-4 h-4 text-green-400" /> : <Copy className="w-4 h-4 text-gray-400" />}
    </button>
  )
}

function CodeBlock({ code }: { code: string; language?: string }) {
  return (
    <div className="relative">
      <pre className="bg-[#0f0f0f] border border-[#222] rounded-lg p-4 overflow-x-auto text-sm">
        <code className="text-gray-300">{code}</code>
      </pre>
      <CopyButton text={code} />
    </div>
  )
}

function SectionHeader({ id, children }: { id: string; children: React.ReactNode }) {
  return (
    <h2 id={id} className="text-2xl font-bold text-white mb-6 scroll-mt-20">
      {children}
    </h2>
  )
}

function SubHeader({ id, children }: { id: string; children: React.ReactNode }) {
  return (
    <h3 id={id} className="text-lg font-semibold text-white mt-8 mb-4 scroll-mt-20">
      {children}
    </h3>
  )
}

function KeybindSection({ id, title, binds }: { id: string; title: string; binds: { key: string; action: string }[] }) {
  return (
    <div id={id} className="scroll-mt-20">
      <h4 className="text-sm font-medium text-green-400 mb-3">{title}</h4>
      <div className="bg-[#0f0f0f] border border-[#222] rounded-lg overflow-hidden">
        <table className="w-full text-sm">
          <tbody>
            {binds.map((bind, i) => (
              <tr key={i} className="border-b border-[#1a1a1a] last:border-0">
                <td className="px-4 py-2.5 text-gray-300 font-mono w-28">{bind.key}</td>
                <td className="px-4 py-2.5 text-gray-400">{bind.action}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}

function InstallSection() {
  const [activeTab, setActiveTab] = useState("script")
  const activeCommand = INSTALL_COMMANDS.find((c) => c.id === activeTab)?.command || ""

  return (
    <div className="flex flex-col items-start gap-2">
      <div className="flex gap-1 bg-[#111] rounded-t-lg p-1">
        {INSTALL_COMMANDS.map((cmd) => (
          <button
            key={cmd.id}
            onClick={() => setActiveTab(cmd.id)}
            className={`flex items-center gap-2 px-4 py-2 text-sm font-medium transition-colors rounded ${
              activeTab === cmd.id
                ? "text-green-400 bg-[#0f0f0f]"
                : "text-gray-400 hover:text-gray-200"
            }`}
          >
            <cmd.icon className="w-4 h-4" />
            {cmd.label}
          </button>
        ))}
      </div>
      <CodeBlock code={`$ ${activeCommand}`} language="bash" />
    </div>
  )
}

function FeaturesGrid() {
  return (
    <div className="grid md:grid-cols-2 gap-4">
      {FEATURES.map((feature) => (
        <div
          key={feature.title}
          className="bg-[#0f0f0f] border border-[#222] rounded-lg p-5 hover:border-green-500/30 transition-colors"
        >
          <feature.icon className="w-6 h-6 text-green-400 mb-3" />
          <h3 className="text-white font-medium mb-2">{feature.title}</h3>
          <p className="text-gray-400 text-sm leading-relaxed">{feature.description}</p>
        </div>
      ))}
    </div>
  )
}

function EnvVarsTable() {
  return (
    <div className="bg-[#0f0f0f] border border-[#222] rounded-lg overflow-hidden">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-[#222]">
            <th className="px-4 py-3 text-left text-gray-300 font-medium">Variable</th>
            <th className="px-4 py-3 text-left text-gray-300 font-medium">Description</th>
          </tr>
        </thead>
        <tbody>
          {ENV_VARS.map((v, i) => (
            <tr key={i} className="border-b border-[#1a1a1a] last:border-0">
              <td className="px-4 py-2.5 text-green-400 font-mono">{v.name}</td>
              <td className="px-4 py-2.5 text-gray-400">{v.description}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function ArchitectureSection() {
  return (
    <div className="bg-[#0f0f0f] border border-[#222] rounded-lg overflow-hidden">
      <table className="w-full text-sm">
        <tbody>
          {ARCHITECTURE.map((item, i) => (
            <tr key={i} className="border-b border-[#1a1a1a] last:border-0">
              <td className="px-4 py-2.5 text-green-400 font-mono">{item.path}</td>
              <td className="px-4 py-2.5 text-gray-400">{item.desc}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function GitProvidersSection() {
  return (
    <div className="space-y-6">
      {GIT_PROVIDERS.map((provider) => (
        <div key={provider.name} className="bg-[#0f0f0f] border border-[#222] rounded-lg p-5">
          <h4 className="text-white font-medium mb-3">{provider.name}</h4>
          <ul className="text-gray-400 text-sm mb-4 space-y-1">
            {provider.features.map((f, i) => (
              <li key={i} className="flex items-center gap-2">
                <span className="text-green-400">•</span> {f}
              </li>
            ))}
          </ul>
          <CodeBlock code={provider.config} />
        </div>
      ))}
    </div>
  )
}

function PMProvidersSection() {
  return (
    <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
      {PM_PROVIDERS.map((provider) => (
        <div key={provider.name} className="bg-[#0f0f0f] border border-[#222] rounded-lg p-4">
          <h4 className="text-white font-medium mb-2">{provider.name}</h4>
          <ul className="text-gray-400 text-sm space-y-1">
            {provider.features.map((f, i) => (
              <li key={i} className="flex items-center gap-2">
                <span className="text-green-400">•</span> {f}
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  )
}

export default function DocsPage() {
  return (
    <DocsLayout>
      <div className="max-w-3xl">
        <div className="mb-12">
          <h1 className="text-3xl font-bold text-white mb-4">Documentation</h1>
          <p className="text-gray-400 text-lg">
            Grove is a terminal UI for managing multiple AI coding agents with git worktree isolation.
            Built with Rust using ratatui for the UI and tokio for async operations.
          </p>
        </div>

        <section id="installation" className="mb-16 scroll-mt-20">
          <SectionHeader id="installation">Installation</SectionHeader>
          <p className="text-gray-400 mb-6">
            Choose your preferred installation method. The install script is recommended for most users.
          </p>
          <InstallSection />
        </section>

        <section id="features" className="mb-16 scroll-mt-20">
          <SectionHeader id="features">Features</SectionHeader>
          <p className="text-gray-400 mb-6">
            Powerful features designed for developers who want to leverage multiple AI coding assistants simultaneously.
          </p>
          <FeaturesGrid />
        </section>

        <section id="keybinds" className="mb-16 scroll-mt-20">
          <SectionHeader id="keybinds">Keybinds</SectionHeader>
          <p className="text-gray-400 mb-6">
            All keybinds are customizable in <code className="text-green-400">~/.grove/config.toml</code>.
            Below are the default bindings.
          </p>

          <div className="space-y-6">
            <KeybindSection id="keybinds-navigation" title="Navigation" binds={KEYBINDS.navigation} />
            <KeybindSection id="keybinds-agent" title="Agent Management" binds={KEYBINDS.agent} />
            <KeybindSection id="keybinds-git" title="Git Operations" binds={KEYBINDS.git} />
            <KeybindSection id="keybinds-view" title="View Controls" binds={KEYBINDS.view} />
            <KeybindSection id="keybinds-external" title="External Services" binds={KEYBINDS.external} />
            <KeybindSection id="keybinds-pm" title="Project Management" binds={KEYBINDS.pm} />
            <KeybindSection id="keybinds-devserver" title="Dev Server" binds={KEYBINDS.devserver} />
            <KeybindSection id="keybinds-other" title="Other" binds={KEYBINDS.other} />
          </div>
        </section>

        <section id="configuration" className="mb-16 scroll-mt-20">
          <SectionHeader id="configuration">Configuration</SectionHeader>
          <p className="text-gray-400 mb-6">
            Grove uses a two-level configuration system: global user preferences and per-repo project settings.
          </p>

          <SubHeader id="config-global">Global Config</SubHeader>
          <p className="text-gray-400 text-sm mb-4">
            User preferences stored in <code className="text-green-400">~/.grove/config.toml</code>:
          </p>
          <CodeBlock code={GLOBAL_CONFIG} />

          <SubHeader id="config-repo">Repo Config</SubHeader>
          <p className="text-gray-400 text-sm mb-4">
            Project-specific settings in <code className="text-green-400">.grove/project.toml</code> (can be committed):
          </p>
          <CodeBlock code={REPO_CONFIG} />

          <SubHeader id="config-env">Environment Variables</SubHeader>
          <p className="text-gray-400 text-sm mb-4">
            API tokens are read from environment variables (never stored in config files):
          </p>
          <EnvVarsTable />
        </section>

        <section id="integrations" className="mb-16 scroll-mt-20">
          <SectionHeader id="integrations">Integrations</SectionHeader>

          <SubHeader id="integrations-git">Git Providers</SubHeader>
          <p className="text-gray-400 text-sm mb-4">
            Grove supports GitLab, GitHub, and Codeberg with full PR/MR and CI/CD integration:
          </p>
          <GitProvidersSection />

          <SubHeader id="integrations-pm">Project Management</SubHeader>
          <p className="text-gray-400 text-sm mb-4">
            Connect agents to tasks in your project management tool:
          </p>
          <PMProvidersSection />
        </section>

        <section id="architecture" className="mb-16 scroll-mt-20">
          <SectionHeader id="architecture">Architecture</SectionHeader>
          <p className="text-gray-400 mb-6">
            Grove is built with Rust using ratatui for the terminal UI, tokio for async runtime,
            and git2 for git operations. All state mutations flow through an action-based state management pattern.
          </p>
          <ArchitectureSection />

          <h4 className="text-sm font-medium text-green-400 mt-8 mb-3">Key Dependencies</h4>
          <div className="bg-[#0f0f0f] border border-[#222] rounded-lg overflow-hidden">
            <table className="w-full text-sm">
              <tbody>
                <tr className="border-b border-[#1a1a1a]">
                  <td className="px-4 py-2.5 text-green-400 font-mono">ratatui</td>
                  <td className="px-4 py-2.5 text-gray-400">Terminal UI rendering</td>
                </tr>
                <tr className="border-b border-[#1a1a1a]">
                  <td className="px-4 py-2.5 text-green-400 font-mono">crossterm</td>
                  <td className="px-4 py-2.5 text-gray-400">Terminal events</td>
                </tr>
                <tr className="border-b border-[#1a1a1a]">
                  <td className="px-4 py-2.5 text-green-400 font-mono">tokio</td>
                  <td className="px-4 py-2.5 text-gray-400">Async runtime</td>
                </tr>
                <tr className="border-b border-[#1a1a1a]">
                  <td className="px-4 py-2.5 text-green-400 font-mono">git2</td>
                  <td className="px-4 py-2.5 text-gray-400">Git operations</td>
                </tr>
                <tr className="border-b border-[#1a1a1a]">
                  <td className="px-4 py-2.5 text-green-400 font-mono">anyhow</td>
                  <td className="px-4 py-2.5 text-gray-400">Error handling</td>
                </tr>
                <tr>
                  <td className="px-4 py-2.5 text-green-400 font-mono">serde</td>
                  <td className="px-4 py-2.5 text-gray-400">Serialization</td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>
      </div>
    </DocsLayout>
  )
}
