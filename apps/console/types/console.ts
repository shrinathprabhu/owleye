export type ConsoleRole = "admin" | "owner" | "viewer";

export type ConsoleUserSummary = {
  is_admin?: boolean;
  email: string;
  name?: string | null;
};

export type ConsoleSelectOption = {
  badge?: string;
  description?: string;
  disabled?: boolean;
  label: string;
  meta?: string;
  value: number | string;
};
