import { Navigate, Route, Routes } from "react-router-dom";
import { Sidebar } from "@/components/Sidebar";
import { Header } from "@/components/Header";
import { Toaster } from "@/components/ui/sonner";
import { Dashboard } from "@/routes/Dashboard";
import { Jobs } from "@/routes/Jobs";
import { JobNew } from "@/routes/JobNew";
import { JobDetail } from "@/routes/JobDetail";
import { Trucks } from "@/routes/Trucks";
import { TruckDetail } from "@/routes/TruckDetail";
import { Settings } from "@/routes/Settings";

function App() {
  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden">
        <Header />
        <main className="flex-1 overflow-auto p-6">
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
        </main>
      </div>
      <Toaster richColors closeButton />
    </div>
  );
}

export default App;
