import type { ReactNode } from 'react';

// RapidRAW's cloud sign-in is not part of Grain Studio: its provider option is switched off in Settings and the
// service behind it isn't ours. These stand in for the sign-in library, so the cloud code that is still in the
// tree keeps compiling, always sees "signed out", and nothing is loaded from the web at startup.
export const useUser = () => ({ user: null as any, isSignedIn: false });
export const useAuth = () => ({ getToken: async (): Promise<string | null> => null });
export const useClerk = () => ({ signOut: async () => {} });
export const Show = ({ when, children }: { when: string; children: ReactNode }) =>
  when === 'signed-out' ? <>{children}</> : null;
export const SignIn = (_props: Record<string, unknown>) => null;
