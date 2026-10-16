import { defineConfig } from "../../config/index.ts";

export default defineConfig({ base: process.env.OBOP_ADMIN_BASE ?? "/" });
