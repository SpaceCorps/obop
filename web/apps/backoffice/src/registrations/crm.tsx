// CRM: "New quotation" on an opportunity, which opens the quotation in Sales (UI plan 3.3).
import { registerRecordExtension } from "../extensions.ts";
import { QuoteFromLead } from "../apps/crm/QuoteFromLead.tsx";

registerRecordExtension("lead", { actions: [QuoteFromLead] });
