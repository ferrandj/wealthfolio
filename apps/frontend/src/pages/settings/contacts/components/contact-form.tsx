import {
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Textarea,
} from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import {
  CONTACT_KINDS,
  CONTACT_ROLES,
  hasPersonNames,
  joinFirstNames,
  splitFirstNames,
  type ContactForm as ContactFormValue,
} from "../contacts-utils";

interface Props {
  value: ContactFormValue;
  onChange: (value: ContactFormValue) => void;
  /** The kind is already known (chosen in an earlier step): leave the choice out. */
  hideKind?: boolean;
}

/** The fields of a contact's card: who it is, how to reach it, and whether it is paid or owes. */
export function ContactForm({ value, onChange, hideKind = false }: Props) {
  const { t } = useTranslation();
  const set = <K extends keyof ContactFormValue>(key: K, v: ContactFormValue[K]) =>
    onChange({ ...value, [key]: v });
  const named = hasPersonNames(value.kind);
  const kindItems = CONTACT_KINDS.map((kind) => ({
    value: kind,
    label: t(`settings:contacts.kind_${kind}`),
  }));
  const firstNames = splitFirstNames(value.firstName);
  const setFirstName = (index: number, name: string) =>
    set(
      "firstName",
      joinFirstNames(firstNames.map((n, i) => (i === index ? name.replace(/\s+/g, "") : n))),
    );
  const roleItems = CONTACT_ROLES.map((role) => ({
    value: role,
    label: t(`settings:contacts.role_${role}`),
  }));

  return (
    <div className="grid gap-4">
      <div className="grid gap-4 sm:grid-cols-2">
        {!hideKind && (
          <div className="grid gap-1.5">
            <Label>{t("settings:contacts.field_kind")}</Label>
            <Select
              items={kindItems}
              value={value.kind}
              onValueChange={(v) => v && set("kind", v as ContactFormValue["kind"])}
            >
              <SelectTrigger aria-label={t("settings:contacts.field_kind")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {CONTACT_KINDS.map((kind) => (
                  <SelectItem key={kind} value={kind}>
                    {t(`settings:contacts.kind_${kind}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}
        <div className="grid gap-1.5">
          <Label>{t("settings:contacts.field_role")}</Label>
          <Select
            items={roleItems}
            value={value.role}
            onValueChange={(v) => v && set("role", v as ContactFormValue["role"])}
          >
            <SelectTrigger aria-label={t("settings:contacts.field_role")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {CONTACT_ROLES.map((role) => (
                <SelectItem key={role} value={role}>
                  {t(`settings:contacts.role_${role}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {named ? (
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="grid gap-1.5">
            <Label htmlFor="contact-civility">{t("settings:contacts.field_civility")}</Label>
            <Input
              id="contact-civility"
              value={value.civility}
              placeholder={t("settings:contacts.civility_placeholder")}
              onChange={(e) => set("civility", e.target.value)}
            />
          </div>
          {firstNames.map((name, index) => (
            <div key={index} className="grid gap-1.5">
              <Label htmlFor={`contact-first-name-${index}`}>
                {t(
                  index === 0
                    ? "settings:contacts.field_first_name"
                    : "settings:contacts.field_first_name_n",
                  { n: index + 1 },
                )}
              </Label>
              <Input
                id={`contact-first-name-${index}`}
                value={name}
                onChange={(e) => setFirstName(index, e.target.value)}
              />
            </div>
          ))}
          <div className="grid gap-1.5">
            <Label htmlFor="contact-last-name">{t("settings:contacts.field_last_name")}</Label>
            <Input
              id="contact-last-name"
              value={value.lastName}
              onChange={(e) => set("lastName", e.target.value)}
            />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="contact-birth-name">{t("settings:contacts.field_birth_name")}</Label>
            <Input
              id="contact-birth-name"
              value={value.birthName}
              onChange={(e) => set("birthName", e.target.value)}
            />
          </div>
          <div className="grid gap-1.5 sm:col-span-2">
            <Label htmlFor="contact-display-name">
              {t("settings:contacts.field_display_name")}
            </Label>
            <Input
              id="contact-display-name"
              value={value.displayName}
              placeholder={t("settings:contacts.display_name_auto")}
              onChange={(e) => set("displayName", e.target.value)}
            />
          </div>
        </div>
      ) : (
        <div className="grid gap-1.5">
          <Label htmlFor="contact-display-name">{t("settings:contacts.field_name")}</Label>
          <Input
            id="contact-display-name"
            value={value.displayName}
            onChange={(e) => set("displayName", e.target.value)}
          />
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="grid gap-1.5">
          <Label htmlFor="contact-phone">{t("settings:contacts.field_phone")}</Label>
          <Input
            id="contact-phone"
            inputMode="tel"
            value={value.phone}
            onChange={(e) => set("phone", e.target.value)}
          />
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="contact-email">{t("settings:contacts.field_email")}</Label>
          <Input
            id="contact-email"
            type="email"
            value={value.email}
            onChange={(e) => set("email", e.target.value)}
          />
        </div>
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="contact-address">{t("settings:contacts.field_address")}</Label>
        <Textarea
          id="contact-address"
          rows={2}
          value={value.address}
          onChange={(e) => set("address", e.target.value)}
        />
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="contact-notes">{t("settings:contacts.field_notes")}</Label>
        <Textarea
          id="contact-notes"
          rows={2}
          value={value.notes}
          onChange={(e) => set("notes", e.target.value)}
        />
      </div>
    </div>
  );
}
