// SPDX-License-Identifier: GPL-3.0-or-later
import { forwardRef, useState } from "react";
import { parseAddresses } from "./articles";

interface Props {
  onAdd: (urls: string[]) => void;
  onCancel: () => void;
}

/** Paste one or more web addresses; each becomes an article on the Books tab. */
const ArticleField = forwardRef<HTMLInputElement, Props>(function ArticleField({ onAdd, onCancel }, ref) {
  const [text, setText] = useState("");
  const [invalid, setInvalid] = useState(false);
  const add = () => {
    const urls = parseAddresses(text);
    if (urls.length === 0) {
      setInvalid(true);
      return;
    }
    onAdd(urls);
  };
  return (
    <form
      className="article-field"
      onSubmit={(e) => {
        e.preventDefault();
        add();
      }}
    >
      <div className="article-row">
        <input
          ref={ref}
          type="text"
          autoFocus
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setInvalid(false);
          }}
          onKeyDown={(e) => e.key === "Escape" && onCancel()}
          placeholder="Paste a web address"
          aria-label="Web address"
          aria-invalid={invalid}
          spellCheck={false}
        />
        <button type="submit" className="ghost">
          Add
        </button>
        <button type="button" className="ghost" onClick={onCancel}>
          Cancel
        </button>
      </div>
      {invalid && <div className="article-error">That isn't a web address</div>}
    </form>
  );
});

export default ArticleField;
