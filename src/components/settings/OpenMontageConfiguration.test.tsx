import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { OpenMontageConfiguration } from "./OpenMontageConfiguration";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));
const invokeMock = vi.mocked(invoke);
const configuration = {
  allowPaidTools: false, allowModelDownloads: false,
  credentials: [{ key: "OPENAI_API_KEY", label: "OpenAI", secret: true, configured: false }, { key: "ELEVENLABS_API_KEY", label: "ElevenLabs", secret: true, configured: true }],
  optionalPackages: [{ id: "piper", label: "Narração local · Piper", installed: true }, { id: "analysis", label: "Análise de vídeo", installed: false }],
};
beforeEach(() => invokeMock.mockReset().mockResolvedValue(configuration));

it("accepts a provider endpoint as text while keeping credential values hidden", async () => {
  invokeMock.mockResolvedValue({ ...configuration, credentials: [...configuration.credentials, { key: "OPENAI_BASE_URL", label: "Endpoint OpenAI", secret: false, configured: true }] });
  render(<OpenMontageConfiguration open onOpenChange={vi.fn()} onSaved={vi.fn()} />);
  expect(await screen.findByLabelText("Endpoint OpenAI")).toHaveAttribute("type", "text");
  expect(screen.getByLabelText("Endpoint OpenAI")).toHaveValue("");
  expect(screen.getByLabelText("OpenAI")).toHaveAttribute("type", "password");
  expect(screen.getByRole("button", { name: "Remover configuração de Endpoint OpenAI" })).toBeVisible();
});

it("distinguishes configured API credentials from optional local dependencies without revealing secrets", async () => {
  render(<OpenMontageConfiguration open onOpenChange={vi.fn()} onSaved={vi.fn()} />);
  expect(await screen.findByText("Configuração das ferramentas")).toBeVisible();
  const credentials = screen.getByRole("region", { name: "Credenciais do OpenMontage" });
  expect(within(credentials).getByText("Não configurada")).toBeVisible();
  expect(within(credentials).getByText("Configurada")).toBeVisible();
  expect(screen.getByLabelText("OpenAI")).toHaveAttribute("type", "password");
  expect(screen.getByLabelText("ElevenLabs")).toHaveValue("");
  expect(screen.getByText("Dependências prontas")).toBeVisible();
  expect(screen.getByRole("button", { name: "Instalar Análise de vídeo" })).toBeEnabled();
  expect(screen.getByRole("switch", { name: "Permitir ferramentas de API com cobrança" })).not.toBeChecked();
  expect(screen.getByRole("switch", { name: "Permitir downloads de modelos" })).not.toBeChecked();
  expect(screen.getByText(/As assinaturas usadas para conversar/)).toBeVisible();
  expect(invokeMock).toHaveBeenCalledTimes(1);
  expect(invokeMock).toHaveBeenCalledWith("get_openmontage_configuration");
});

it("preserves entered credentials after a vault failure and only sends the explicit replacement on retry", async () => {
  const user = userEvent.setup();
  const close = vi.fn(), saved = vi.fn().mockResolvedValue(undefined);
  let fail = true;
  invokeMock.mockImplementation(async command => {
    if (command === "save_openmontage_configuration" && fail) throw { message: "Desbloqueie o cofre de credenciais." };
    return configuration;
  });
  render(<OpenMontageConfiguration open onOpenChange={close} onSaved={saved} />);
  await user.type(await screen.findByLabelText("OpenAI"), "test-only-key");
  await user.click(screen.getByRole("switch", { name: "Permitir ferramentas de API com cobrança" }));
  await user.click(screen.getByRole("button", { name: "Salvar configuração" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Desbloqueie o cofre");
  expect(screen.getByLabelText("OpenAI")).toHaveValue("test-only-key");
  expect(close).not.toHaveBeenCalled();
  fail = false;
  await user.click(screen.getByRole("button", { name: "Salvar configuração" }));
  await waitFor(() => expect(close).toHaveBeenCalledWith(false));
  expect(invokeMock).toHaveBeenLastCalledWith("save_openmontage_configuration", { configuration: { allowPaidTools: true, allowModelDownloads: false, credentials: { OPENAI_API_KEY: "test-only-key" }, removeCredentials: [] } });
  expect(saved).toHaveBeenCalledOnce();
  expect(screen.getByLabelText("OpenAI")).toHaveValue("");
});

it("removes only the selected saved credential when the configuration is saved", async () => {
  const user = userEvent.setup();
  render(<OpenMontageConfiguration open onOpenChange={vi.fn()} onSaved={vi.fn().mockResolvedValue(undefined)} />);
  await user.type(await screen.findByLabelText("ElevenLabs"), "unused-replacement");
  await user.click(screen.getByRole("button", { name: "Remover chave de ElevenLabs" }));
  expect(screen.getByText("Será removida")).toBeVisible();
  expect(screen.getByLabelText("ElevenLabs")).toBeDisabled();
  expect(invokeMock).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole("button", { name: "Salvar configuração" }));
  expect(invokeMock).toHaveBeenLastCalledWith("save_openmontage_configuration", { configuration: { allowPaidTools: false, allowModelDownloads: false, credentials: {}, removeCredentials: ["ELEVENLABS_API_KEY"] } });
});

it("installs an optional resource only on request and preserves unsaved permissions", async () => {
  const user = userEvent.setup();
  invokeMock.mockImplementation(async command => command === "install_openmontage_optional_package" ? { ...configuration, optionalPackages: configuration.optionalPackages.map(pkg => ({ ...pkg, installed: true })) } : configuration);
  render(<OpenMontageConfiguration open onOpenChange={vi.fn()} onSaved={vi.fn().mockResolvedValue(undefined)} />);
  await user.click(await screen.findByRole("switch", { name: "Permitir downloads de modelos" }));
  const install = screen.getByRole("button", { name: "Instalar Análise de vídeo" });
  fireEvent.click(install); fireEvent.click(install);
  await waitFor(() => expect(screen.queryByRole("button", { name: "Instalar Análise de vídeo" })).not.toBeInTheDocument());
  expect(invokeMock.mock.calls.filter(([command]) => command === "install_openmontage_optional_package")).toEqual([["install_openmontage_optional_package", { id: "analysis" }]]);
  expect(screen.getByRole("switch", { name: "Permitir downloads de modelos" })).toBeChecked();
  await user.click(screen.getByRole("button", { name: "Salvar configuração" }));
  expect(invokeMock).toHaveBeenLastCalledWith("save_openmontage_configuration", { configuration: { allowPaidTools: false, allowModelDownloads: true, credentials: {}, removeCredentials: [] } });
});

it("keeps Manim unavailable after failure and preserves unsaved configuration through a successful retry", async () => {
  const user = userEvent.setup();
  const saved = vi.fn().mockResolvedValue(undefined);
  const animationConfiguration = { ...configuration, optionalPackages: [{ id: "animation", label: "Animações Manim (Cairo e Pango incluídos)", installed: false }] };
  let finishInstallation: (value: typeof animationConfiguration) => void = () => { throw new Error("Installation has not started"); };
  const pendingInstallation = new Promise<typeof animationConfiguration>(resolve => { finishInstallation = resolve; });
  invokeMock.mockResolvedValueOnce(animationConfiguration)
    .mockRejectedValueOnce({ message: "Não foi possível preparar Cairo e Pango." })
    .mockReturnValueOnce(pendingInstallation);
  render(<OpenMontageConfiguration open onOpenChange={vi.fn()} onSaved={saved} />);
  await user.type(await screen.findByLabelText("OpenAI"), "test-only-key");
  await user.click(screen.getByRole("switch", { name: "Permitir ferramentas de API com cobrança" }));
  await user.click(screen.getByRole("switch", { name: "Permitir downloads de modelos" }));
  const resources = screen.getByRole("region", { name: "Recursos locais do OpenMontage" });
  expect(within(resources).getByText("Animações Manim (Cairo e Pango incluídos)")).toBeVisible();
  await user.click(within(resources).getByRole("button", { name: "Instalar Animações Manim (Cairo e Pango incluídos)" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível preparar Cairo e Pango.");
  expect(within(resources).queryByText("Dependências prontas")).not.toBeInTheDocument();
  expect(saved).not.toHaveBeenCalled();
  await user.click(within(resources).getByRole("button", { name: "Instalar Animações Manim (Cairo e Pango incluídos)" }));
  expect(within(resources).getByRole("button", { name: "Instalar Animações Manim (Cairo e Pango incluídos)" })).toBeDisabled();
  expect(within(resources).queryByText("Dependências prontas")).not.toBeInTheDocument();
  finishInstallation({ ...animationConfiguration, optionalPackages: animationConfiguration.optionalPackages.map(pkg => ({ ...pkg, installed: true })) });
  expect(await within(resources).findByText("Dependências prontas")).toBeVisible();
  expect(saved).toHaveBeenCalledOnce();
  expect(screen.getByLabelText("OpenAI")).toHaveValue("test-only-key");
  expect(screen.getByRole("switch", { name: "Permitir ferramentas de API com cobrança" })).toBeChecked();
  expect(screen.getByRole("switch", { name: "Permitir downloads de modelos" })).toBeChecked();
  expect(invokeMock.mock.calls.filter(([command]) => command === "install_openmontage_optional_package")).toEqual([
    ["install_openmontage_optional_package", { id: "animation" }],
    ["install_openmontage_optional_package", { id: "animation" }],
  ]);
});

it("offers retry when configuration cannot be loaded and does not enable save prematurely", async () => {
  const user = userEvent.setup();
  invokeMock.mockRejectedValueOnce({ message: "Não foi possível acessar o cofre." }).mockResolvedValue(configuration);
  render(<OpenMontageConfiguration open onOpenChange={vi.fn()} onSaved={vi.fn()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível acessar o cofre");
  expect(screen.getByRole("button", { name: "Salvar configuração" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Tentar novamente" }));
  expect(await screen.findByLabelText("OpenAI")).toBeVisible();
  expect(screen.getByRole("button", { name: "Salvar configuração" })).toBeEnabled();
});
