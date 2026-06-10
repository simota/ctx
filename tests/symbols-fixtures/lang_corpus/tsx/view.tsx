// TSX fixture: TS kinds (function/class/interface/type) over the TSX grammar.
import * as React from "react";

export interface ViewProps<T> {
  items: T[];
  onSelect: (item: T) => void;
}

export type ViewMode = "list" | "grid";

export function View<T>(props: ViewProps<T>): JSX.Element {
  function renderItem(item: T): JSX.Element {
    return <li>{String(item)}</li>;
  }
  return <ul>{props.items.map(renderItem)}</ul>;
}

export class Container<T> extends React.Component<ViewProps<T>> {
  private mode: ViewMode = "list";
  render(): JSX.Element {
    return <View {...this.props} />;
  }
}

interface InternalState {
  open: boolean;
}

type Handler = (e: Event) => void;
