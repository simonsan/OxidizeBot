import React from "react";
import {Form, Button, InputGroup, Row, Col} from "react-bootstrap";
import {FontAwesomeIcon} from "@fortawesome/react-fontawesome";
import {True, False} from "../../utils";

class Base {
  edit() {
    throw new Error("missing edit() implementation");
  }

  hasEditControl() {
    return true;
  }
}

/**
 * Decode the given type and value.
 *
 * @param {object} type the type to decode
 * @param {any} value the value to decode
 */
export function decode(type) {
  if (type === null) {
    return RawType;
  }

  switch (type.id) {
    case "duration":
      return DurationType;
    case "bool":
      return BooleanType;
    case "string":
      return StringType;
    case "number":
      return NumberType;
    case "set":
      let value = decode(type.value);
      return new SetType(value);
    default:
      return RawType;
  }
}

const DURATION_REGEX = /^((\d+)d)?((\d+)h)?((\d+)m)?((\d+)s)?$/;

class EditDuration {
  validate(value) {
    return (
      value.days >= 0 &&
      value.hours >= 0 && value.hours < 24 &&
      value.minutes >= 0 && value.minutes < 60 &&
      value.seconds >= 0 && value.seconds < 60
    );
  }

  save(value) {
    return {
      control: new Duration(),
      value: Object.assign(value, {}),
    };
  }

  control(_isValid, value, onChange) {
    let days = this.digitControl(
      value.days, "d", v => onChange(Object.assign(value, {days: v})), _ => true
    );
    let hours = this.digitControl(
      value.hours, "h", v => onChange(Object.assign(value, {hours: v})), v => v >= 0 && v < 24
    );
    let minutes = this.digitControl(
      value.minutes, "m", v => onChange(Object.assign(value, {minutes: v})), v => v >= 0 && v < 60
    );
    let seconds = this.digitControl(
      value.seconds, "s", v => onChange(Object.assign(value, {seconds: v})), v => v >= 0 && v < 60
    );

    return (
      <Row>
        <Col>
          {days}
        </Col>

        <Col>
          {hours}
        </Col>

        <Col>
          {minutes}
        </Col>

        <Col>
          {seconds}
        </Col>
      </Row>
    );
  }

  digitControl(value, suffix, onChange, validate) {
    var isValid = validate(value);

    return (
      <InputGroup size="sm">
        <Form.Control type="number" value={value} isInvalid={!isValid} onChange={
          e => {
            onChange(parseInt(e.target.value) || 0);
          }
        } />

        <InputGroup.Append>
          <InputGroup.Text>{suffix}</InputGroup.Text>
        </InputGroup.Append>
      </InputGroup>
    );
  }
}

class DurationType {
  static default() {
    return {days: 0, hours: 0, minutes: 0, seconds: 1};
  }

  static construct(data) {
    return Duration.parse(data);
  }
}

export class Duration extends Base {
  /**
   * Parse the given duration.
   *
   * @param {string} input input to parse.
   */
  static parse(input) {
    let m = DURATION_REGEX.exec(input);

    if (!m) {
      return null;
    }

    let days = 0;
    let hours = 0;
    let minutes = 0;
    let seconds = 0;

    if (!!m[2]) {
      days = parseInt(m[2]);
    }

    if (!!m[4]) {
      hours = parseInt(m[4]);
    }

    if (!!m[6]) {
      minutes = parseInt(m[6]);
    }

    if (!!m[8]) {
      seconds = parseInt(m[8]);
    }

    return {
      control: new Duration(),
      value: {days, hours, minutes, seconds},
    };
  }

  render(value) {
    return <code>{this.convertToString(value)}</code>;
  }

  edit(editValue) {
    return {
      edit: new EditDuration(),
      editValue,
    };
  }

  /**
   * Serialize to remote representation.
   */
  serialize(value) {
    return this.convertToString(value);
  }

  /**
   * Convert the duration into a string.
   */
  convertToString(value) {
    let nothing = true;
    let s = "";

    if (value.days > 0) {
      nothing = false;
      s += `${value.days}d`;
    }

    if (value.hours > 0) {
      nothing = false;
      s += `${value.hours}h`;
    }

    if (value.minutes > 0) {
      nothing = false;
      s += `${value.minutes}m`;
    }

    if (value.seconds > 0 || nothing) {
      s += `${value.seconds}s`;
    }

    return s;
  }
}

class EditNumber {
  validate(value) {
    return true;
  }

  save(value) {
    return {
      control: new Number(),
      value,
    };
  }

  control(isValid, value, onChange) {
    return <Form.Control size="sm" type="number" isInvalid={!isValid} value={value} onChange={
      e => {
        onChange(parseInt(e.target.value) || 0);
      }
    } />
  }
}

class NumberType {
  static default() {
    return 0;
  }

  static construct(data) {
    return {
      control: new Number(),
      value: data,
    };
  }
}

export class Number extends Base {
  static parse(input) {
    return {
      render: new Number(),
      value: parseInt(input) || 0,
    };
  }

  render(value) {
    return value.toString();
  }

  edit(editValue) {
    return {
      edit: new EditNumber(),
      editValue,
    };
  }

  serialize(value) {
    return value;
  }
}

export class BooleanType {
  static default() {
    return false;
  }

  static construct(value) {
    return {
      control: new Boolean(),
      value,
    };
  }
}

export class Boolean extends Base {
  render(value, onChange) {
    if (value) {
      return <Button title="Toggle to false" size="sm" variant="success" onClick={() => onChange(false)}><True /></Button>;
    } else {
      return <Button title="Toggle to true" size="sm" variant="danger" onClick={() => onChange(true)}><False /></Button>;
    }
  }

  hasEditControl() {
    return false;
  }

  serialize(value) {
    return value;
  }
}

class EditString {
  validate(value) {
    return true;
  }

  save(value) {
    return {
      control: new String(),
      value,
    };
  }

  control(_isValid, value, onChange) {
    return <Form.Control size="sm" type="value" value={value} onChange={e => onChange(e.target.value)} />
  }
}

export class StringType {
  static default() {
    return "";
  }

  static construct(value) {
    return {
      control: new String(),
      value,
    };
  }
}

export class String extends Base {
  render(value) {
    return <code>{value}</code>;
  }

  edit(editValue) {
    return {
      edit: new EditString(),
      editValue,
    };
  }

  serialize(value) {
    return value;
  }
}

class EditRaw {
  constructor(value) {
    this.value = value;
  }

  validate(value) {
    try {
      JSON.parse(value);
      return true;
    } catch(e) {
      return false;
    }
  }

  save(value) {
    return {
      control: new Raw(),
      value,
    };
  }

  control(isValid, value, onChange) {
    return <Form.Control size="sm" type="value" isInvalid={!isValid} value={value} onChange={
      e => {
        onChange(e.target.value);
      }
    } />
  }
}

export class RawType {
  static default() {
    return {};
  }

  static construct(value) {
    return {
      control: new Raw(),
      value,
    };
  }
}

export class Raw extends Base {
  static parse(data) {
    return new Raw(JSON.parse(data))
  }

  render(data) {
    return <code>{JSON.stringify(data)}</code>;
  }

  edit(data) {
    return {
      edit: new EditRaw(),
      editValue: JSON.stringify(data),
    };
  }

  serialize(data) {
    return data;
  }
}

class EditSet {
  constructor(type) {
    this.type = type;
  }

  validate(values) {
    return values.every((({edit, editValue}) => edit.validate(editValue)));
  }

  save(values) {
    return {
      control: new Set(this.type),
      value: values.map(({edit, editValue}) => edit.save(editValue)),
    };
  }

  control(_isValid, values, onChange) {
    let add = () => {
      let newValues = values.slice();
      let {control, value} = this.type.construct(this.type.default());
      newValues.push(control.edit(value));
      onChange(newValues);
    };

    let remove = key => _ => {
      let newValues = values.slice();
      newValues.splice(key, 1);
      onChange(newValues);
    };

    return (
      <div>
        {values.map(({edit, editValue}, key) => {
          let isValid = edit.validate(editValue);

          let control = edit.control(isValid, editValue, v => {
            let newValues = values.slice();
            newValues[key].editValue = v;
            onChange(newValues);
          });

          return (
            <InputGroup key={key} className="mb-1">
              {control}
              <InputGroup.Append>
                <Button size="sm" variant="danger" onClick={remove(key)}><FontAwesomeIcon icon="minus" /></Button>
              </InputGroup.Append>
            </InputGroup>
          );
        })}

        <div>
          <Button size="sm" variant="primary" onClick={add}><FontAwesomeIcon icon="plus" /></Button>
        </div>
      </div>
    );
  }
}

export class SetType {
  constructor(type) {
    this.type = type;
  }

  default() {
    return [];
  }

  construct(value) {
    return {
      control: new Set(this.type),
      value: value.map(v => this.type.construct(v)),
    };
  }
}

export class Set extends Base {
  constructor(type) {
    super();
    this.type = type;
  }

  render(values) {
    return (
      <div>
        {values.map(({control, value}, key) => <div key={key}>{control.render(value)}</div>)}
      </div>
    );
  }

  edit(values) {
    return {
      edit: new EditSet(this.type),
      editValue: values.map(({control, value}) => control.edit(value)),
    };
  }

  serialize(values) {
    return values.map(({control, value}) => control.serialize(value));
  }
}