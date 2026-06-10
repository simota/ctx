// JSX fixture: same grammar as JS. function_declaration + class_declaration.
import React from "react";

export function Button(props) {
  function handleClick() {
    return props.onClick();
  }
  return <button onClick={handleClick}>{props.label}</button>;
}

export class Panel extends React.Component {
  render() {
    return <div className="panel">{this.props.children}</div>;
  }
}

function PrivatePanel() {
  return <div />;
}
