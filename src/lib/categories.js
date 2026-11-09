// Display details for each ecosystem. Class names are written out in full so
// Tailwind can find them when it scans the source.
export const CATEGORIES = {
  node: {
    label: "Node.js",
    finds: "node_modules, .next, .nuxt, .angular, dist",
    restore: "npm install, then rebuild",
    dot: "bg-eco-node",
  },
  rust: {
    label: "Rust",
    finds: "target",
    restore: "cargo build",
    dot: "bg-eco-rust",
  },
  python: {
    label: "Python",
    finds: ".venv, __pycache__, tool caches",
    restore: "Recreate the virtualenv and reinstall",
    dot: "bg-eco-python",
  },
  jvm: {
    label: "Java / Kotlin",
    finds: "target, build, .gradle",
    restore: "mvn package or gradle build",
    dot: "bg-eco-jvm",
  },
  dotnet: {
    label: ".NET",
    finds: "bin, obj",
    restore: "dotnet build",
    dot: "bg-eco-dotnet",
  },
  mobile: {
    label: "Flutter / iOS",
    finds: "build, .dart_tool, Pods",
    restore: "flutter pub get or pod install",
    dot: "bg-eco-mobile",
  },
  vendor: {
    label: "Go / PHP / Ruby",
    finds: "vendor, var/cache",
    restore: "go mod vendor, composer install or bundle install",
    dot: "bg-eco-vendor",
  },
  gameEngine: {
    label: "Unity / Unreal",
    finds: "Library, Temp, Intermediate",
    restore: "Reopen the project in the editor (the first import is slow)",
    dot: "bg-eco-game",
  },
  terraform: {
    label: "Terraform",
    finds: ".terraform",
    restore: "terraform init",
    dot: "bg-eco-terraform",
  },
  osJunk: {
    label: "OS junk",
    finds: ".DS_Store, Thumbs.db",
    restore: "Nothing to do. The OS recreates them if needed.",
    dot: "bg-eco-junk",
  },
};

export const CATEGORY_KEYS = Object.keys(CATEGORIES);
