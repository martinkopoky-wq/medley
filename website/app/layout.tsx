import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Medley — Deine Musik spielt zusammen",
  description: "Spotify, SoundCloud und Apple Music in einer nativen App. Dienstübergreifende Mixes, eigene Layouts und Installationsanleitungen für Linux, macOS und Windows.",
  icons: { icon: "/icon.png", shortcut: "/icon.png" },
};
export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return <html lang="de"><body>{children}</body></html>;
}
