/** Tailwind 配置：墨绿 + 纸张 + 赭红，贴合法律卷宗的稳重气质 */
export default {
  content: ["./index.html", "./src/**/*.{ts,html}"],
  // 分类 class 是运行时拼接的（cat-${category}），必须显式保留，否则会被 purge
  safelist: [
    "cat-id_card",
    "cat-phone",
    "cat-landline",
    "cat-bank_card",
    "cat-email",
    "cat-address",
    "cat-person_name",
    "cat-agent",
    "cat-org_name",
    "cat-law_firm",
    "cat-case_number",
    "cat-court",
    "cat-court_code",
    "cat-credit_code",
    "cat-custom",
    "hit",
    "redacted",
    "disabled",
    "manual",
    "active",
  ],
  theme: {
    extend: {
      colors: {
        ink: {
          50: "#f3f7f5",
          100: "#e2ece8",
          200: "#c6d9d3",
          300: "#9fbdb4",
          400: "#6d968c",
          500: "#4a7269",
          600: "#345a52",
          700: "#2a4a43",
          800: "#1b3b36",
          900: "#12241f",
        },
        paper: {
          DEFAULT: "#f7f4ec",
          dark: "#efe9db",
        },
        bronze: {
          50: "#fbf3ee",
          100: "#f4e0d4",
          400: "#c9784f",
          500: "#a34a2a",
          600: "#8a3d22",
        },
      },
      fontFamily: {
        sans: [
          "-apple-system",
          "BlinkMacSystemFont",
          "PingFang SC",
          "Hiragino Sans GB",
          "Microsoft YaHei",
          "Source Han Sans SC",
          "sans-serif",
        ],
        serif: ["Songti SC", "STSong", "SimSun", "serif"],
        mono: ["SF Mono", "Menlo", "Consolas", "monospace"],
      },
      boxShadow: {
        paper: "0 1px 2px rgba(18,36,31,.05), 0 8px 30px rgba(18,36,31,.06)",
        pop: "0 10px 40px rgba(18,36,31,.18)",
      },
      keyframes: {
        flash: {
          "0%": { backgroundColor: "rgba(201,120,79,.45)" },
          "100%": { backgroundColor: "transparent" },
        },
      },
      animation: { flash: "flash 1.4s ease-out" },
    },
  },
  plugins: [],
};
