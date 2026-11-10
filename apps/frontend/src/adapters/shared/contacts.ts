// Contacts: beneficiaries, debtors and people
import type {
  BeneficiarySpending,
  ContactDetail,
  ContactInput,
  ContactSummary,
  MissingOwnAccount,
  NewContact,
  NewContactIdentifier,
  PersonOverview,
  UnresolvedCounterparty,
} from "@/lib/types/contacts";

import { invoke } from "./platform";

export const listContacts = async (): Promise<ContactSummary[]> => {
  return invoke<ContactSummary[]>("list_contacts");
};

export const getContact = async (id: string): Promise<ContactDetail> => {
  return invoke<ContactDetail>("get_contact", { id });
};

export const createContact = async (contact: NewContact): Promise<ContactDetail> => {
  return invoke<ContactDetail>("create_contact", { contact });
};

export const updateContact = async (id: string, input: ContactInput): Promise<ContactDetail> => {
  return invoke<ContactDetail>("update_contact", { id, input });
};

export const deleteContact = async (id: string): Promise<void> => {
  return invoke<void>("delete_contact", { id });
};

export const mergeContacts = async (id: string, targetId: string): Promise<ContactDetail> => {
  return invoke<ContactDetail>("merge_contacts", { id, targetId });
};

export const addContactIdentifier = async (
  id: string,
  identifier: NewContactIdentifier,
): Promise<ContactDetail> => {
  return invoke<ContactDetail>("add_contact_identifier", { id, identifier });
};

export const removeContactIdentifier = async (identifierId: string): Promise<ContactDetail> => {
  return invoke<ContactDetail>("remove_contact_identifier", { identifierId });
};

export const linkContactAccount = async (id: string, accountId: string): Promise<ContactDetail> => {
  return invoke<ContactDetail>("link_contact_account", { id, accountId });
};

export const unlinkContactAccount = async (
  id: string,
  accountId: string,
): Promise<ContactDetail> => {
  return invoke<ContactDetail>("unlink_contact_account", { id, accountId });
};

export const listUnresolvedCounterparties = async (
  includeShops = false,
): Promise<UnresolvedCounterparty[]> => {
  return invoke<UnresolvedCounterparty[]>("list_unresolved_counterparties", { includeShops });
};

export const getSpendingByBeneficiary = async (
  from?: string,
  to?: string,
): Promise<BeneficiarySpending[]> => {
  return invoke<BeneficiarySpending[]>("get_spending_by_beneficiary", { from, to });
};

export const setContactAccountShare = async (
  id: string,
  accountId: string,
  sharePercent: number | string,
): Promise<ContactDetail> => {
  return invoke<ContactDetail>("set_contact_account_share", { id, accountId, sharePercent });
};

export const getPersonOverview = async (
  id: string,
  from?: string,
  to?: string,
): Promise<PersonOverview> => {
  return invoke<PersonOverview>("get_person_overview", { id, from, to });
};

export const listMissingOwnAccounts = async (): Promise<MissingOwnAccount[]> => {
  return invoke<MissingOwnAccount[]>("list_missing_own_accounts");
};
