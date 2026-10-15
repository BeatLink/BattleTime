// The palette's entries, read from the BattleTime SDK's own source so they always match what apps can use.

import * as path from "node:path";
import ts from "typescript";

// ============================================================================
// Entries
// ============================================================================

export type EntryKind = "widget" | "event" | "effect" | "value";

export interface Parameter {
  name: string;
  type: string;
  optional: boolean;
  documentation: string;
}

export interface CatalogEntry {
  kind: EntryKind;
  name: string;
  /** The module an app imports it from, such as `battletime/widgets`; events need `on` from `battletime`. */
  module: string;
  documentation: string;
  /** A widget's props, an effect's arguments, or an event's fields. */
  parameters: Parameter[];
  /** Whether a widget takes children between its tags. */
  takes_children: boolean;
}

export interface Catalog {
  widgets: CatalogEntry[];
  events: CatalogEntry[];
  effects: CatalogEntry[];
  values: CatalogEntry[];
}

const WIDGET_FILES: Record<string, string> = {
  "widgets.tsx": "battletime/widgets",
  "components.tsx": "battletime/components",
};
const INDEX_FILE = "index.ts";

// ============================================================================
// Reading the SDK
// ============================================================================

/** Reads the palette from the SDK's `src` folder, such as `battletime/sdk/battletime/src`. */
export function read_catalog(sdk_source_folder: string): Catalog {
  const files = [...Object.keys(WIDGET_FILES), INDEX_FILE].map((name) => path.join(sdk_source_folder, name));
  const program = ts.createProgram(files, {
    jsx: ts.JsxEmit.Preserve,
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    strict: true,
    noEmit: true,
    skipLibCheck: true,
  });
  const checker = program.getTypeChecker();
  const catalog: Catalog = { widgets: [], events: [], effects: [], values: [] };

  for (const [file_name, module_name] of Object.entries(WIDGET_FILES)) {
    const source = program.getSourceFile(path.join(sdk_source_folder, file_name));
    if (source) catalog.widgets.push(...read_widgets(source, module_name, checker));
  }
  const index = program.getSourceFile(path.join(sdk_source_folder, INDEX_FILE));
  if (index) {
    catalog.events = read_events(index, checker);
    catalog.effects = read_effects(index, checker);
    catalog.values = read_values(index, checker);
  }
  return catalog;
}

function is_exported(node: ts.Node): boolean {
  return ts.canHaveModifiers(node) && (ts.getModifiers(node) ?? []).some((modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword);
}

function documentation_of(symbol: ts.Symbol | undefined, checker: ts.TypeChecker): string {
  return symbol ? ts.displayPartsToString(symbol.getDocumentationComment(checker)) : "";
}

function exported_functions(source: ts.SourceFile): ts.FunctionDeclaration[] {
  return source.statements.filter(
    (statement): statement is ts.FunctionDeclaration => ts.isFunctionDeclaration(statement) && is_exported(statement) && !!statement.name,
  );
}

function parameter_of(property: ts.Symbol, checker: ts.TypeChecker, at: ts.Node): Parameter {
  const optional = (property.flags & ts.SymbolFlags.Optional) !== 0;
  const type = checker.typeToString(checker.getTypeOfSymbolAtLocation(property, at));
  return {
    name: property.getName(),
    // An optional prop's type always ends in `| undefined`, which the `?` already says.
    type: optional ? type.replace(/ \| undefined$/, "") : type,
    optional,
    documentation: documentation_of(property, checker),
  };
}

/** Widgets are the exported functions named with a capital letter, whose one argument is their props. */
function read_widgets(source: ts.SourceFile, module_name: string, checker: ts.TypeChecker): CatalogEntry[] {
  return exported_functions(source)
    .filter((declaration) => /^[A-Z]/.test(declaration.name!.text))
    .map((declaration) => {
      const props = declaration.parameters[0];
      const properties = props ? checker.getTypeAtLocation(props).getProperties() : [];
      const parameters = properties
        .filter((property) => property.getName() !== "children")
        .map((property) => parameter_of(property, checker, declaration));
      return {
        kind: "widget",
        name: declaration.name!.text,
        module: module_name,
        documentation: documentation_of(checker.getSymbolAtLocation(declaration.name!), checker),
        parameters,
        takes_children: properties.some((property) => property.getName() === "children"),
      };
    });
}

/** Events are the members of `EventMap`, with the fields their handler receives. */
function read_events(source: ts.SourceFile, checker: ts.TypeChecker): CatalogEntry[] {
  const event_map = source.statements.find(
    (statement): statement is ts.InterfaceDeclaration => ts.isInterfaceDeclaration(statement) && statement.name.text === "EventMap",
  );
  if (!event_map) return [];
  return event_map.members.filter(ts.isPropertySignature).map((member) => {
    const symbol = checker.getSymbolAtLocation(member.name);
    const event_type = checker.getTypeAtLocation(member);
    return {
      kind: "event",
      name: member.name.getText(source),
      module: "battletime",
      documentation: documentation_of(symbol, checker),
      parameters: event_type.getProperties().map((property) => parameter_of(property, checker, member)),
      takes_children: false,
    };
  });
}

function calls_emit(node: ts.Node): boolean {
  if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "emit") return true;
  return ts.forEachChild(node, calls_emit) ?? false;
}

/** Effects are the exported functions whose body asks the watch to do something through `emit`. */
function read_effects(source: ts.SourceFile, checker: ts.TypeChecker): CatalogEntry[] {
  return exported_functions(source)
    .filter((declaration) => declaration.body && calls_emit(declaration.body))
    .map((declaration) => ({
      kind: "effect",
      name: declaration.name!.text,
      module: "battletime",
      documentation: documentation_of(checker.getSymbolAtLocation(declaration.name!), checker),
      parameters: declaration.parameters.map((parameter) => ({
        name: parameter.name.getText(source),
        type: parameter.type ? parameter.type.getText(source) : checker.typeToString(checker.getTypeAtLocation(parameter)),
        optional: !!parameter.questionToken || !!parameter.initializer,
        documentation: "",
      })),
      takes_children: false,
    }));
}

/** Values are the exported signals the watch keeps up to date, such as `battery_percent`. */
function read_values(source: ts.SourceFile, checker: ts.TypeChecker): CatalogEntry[] {
  const entries: CatalogEntry[] = [];
  for (const statement of source.statements) {
    if (!ts.isVariableStatement(statement) || !is_exported(statement)) continue;
    for (const declaration of statement.declarationList.declarations) {
      const initializer = declaration.initializer;
      const is_signal =
        initializer && ts.isCallExpression(initializer) && ts.isIdentifier(initializer.expression) && ["signal", "computed"].includes(initializer.expression.text);
      if (!is_signal || !ts.isIdentifier(declaration.name)) continue;
      entries.push({
        kind: "value",
        name: declaration.name.text,
        module: "battletime",
        documentation: documentation_of(checker.getSymbolAtLocation(declaration.name), checker),
        parameters: [],
        takes_children: false,
      });
    }
  }
  return entries;
}
