import { lazy, Suspense, useEffect } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { loadSettings, useSettings } from "@/lib/settings";
import { Sidebar } from "@/components/Sidebar";
import { Header } from "@/components/Header";
import { PluginPrompt } from "@/components/PluginPrompt";
import { UpdateBanner } from "@/components/UpdateBanner";
import { Toaster } from "@/components/ui/sonner";
import { TextSkeleton } from "@/components/Skeletons";

// Each page is its own chunk (#4674: one 1.1 MB bundle tripped Vite's 500 kB warning).
// The app loads them from disk, so the wait is a frame or two; preloadPages() fetches them
// all once the first page is up, so moving between pages never waits at all.
const pages = {
  Dashboard: () =>
    import("@/routes/Dashboard").then((m) => ({ default: m.Dashboard })),
  Jobs: () => import("@/routes/Jobs").then((m) => ({ default: m.Jobs })),
  JobNew: () => import("@/routes/JobNew").then((m) => ({ default: m.JobNew })),
  JobDetail: () =>
    import("@/routes/JobDetail").then((m) => ({ default: m.JobDetail })),
  Trucks: () => import("@/routes/Trucks").then((m) => ({ default: m.Trucks })),
  TruckDetail: () =>
    import("@/routes/TruckDetail").then((m) => ({ default: m.TruckDetail })),
  Settings: () =>
    import("@/routes/Settings").then((m) => ({ default: m.Settings })),
};
const Dashboard = lazy(pages.Dashboard);
const Jobs = lazy(pages.Jobs);
const JobNew = lazy(pages.JobNew);
const JobDetail = lazy(pages.JobDetail);
const Trucks = lazy(pages.Trucks);
const TruckDetail = lazy(pages.TruckDetail);
const Settings = lazy(pages.Settings);

function preloadPages() {
  for (const load of Object.values(pages)) {
    load().catch(() => {
      // A failed preload is retried by lazy() when the page is opened.
    });
  }
}

function App() {
  // Subscribing here re-renders every page when the units setting changes.
  useSettings();
  useEffect(() => {
    loadSettings().catch(() => {
      // No backend (plain browser): the defaults stay.
    });
    preloadPages();
  }, []);

  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden">
        <Header />
        <UpdateBanner />
        <PluginPrompt />
        <main className="flex-1 overflow-auto p-6">
          <Suspense fallback={<TextSkeleton />}>
            <Routes>
              <Route path="/" element={<Navigate to="/dashboard" replace />} />
              <Route path="/dashboard" element={<Dashboard />} />
              <Route path="/jobs" element={<Jobs />} />
              <Route path="/jobs/new" element={<JobNew />} />
              <Route path="/jobs/:id" element={<JobDetail />} />
              <Route path="/trucks" element={<Trucks />} />
              <Route path="/trucks/:id" element={<TruckDetail />} />
              <Route path="/settings" element={<Settings />} />
            </Routes>
          </Suspense>
        </main>
      </div>
      <Toaster richColors closeButton />
    </div>
  );
}

export default App;
