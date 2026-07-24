import "./globals.css";
import { TooltipProvider } from "@/components/ui/tooltip";
import { JotaiProvider } from "@/store/provider";
import { ThemeProvider } from "@/components/theme-provider";
import { DashboardPage } from "@/views/dashboard";

export function App() {
  return (
    <ThemeProvider attribute="class" defaultTheme="dark" enableSystem disableTransitionOnChange>
      <JotaiProvider>
        <TooltipProvider>
          <DashboardPage />
        </TooltipProvider>
      </JotaiProvider>
    </ThemeProvider>
  );
}
