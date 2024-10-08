## Generating figures from contender report

The provided python script generates additional figures from the contender report output in csv file.

```sh
pip install absl-py matplotlib pandas seaborn
python generate_figures.py --csv_path <path_to_csv_file> --output_dir <path_to_output_dir>
```

The script generates the following figures:
 - Histogram of Gas Used
 - Average Gas Used per Transaction Type
 - Gas Used vs. Confirmation Time
 - Transaction Count Over Time
 - Distribution of Transaction Types
 - Cumulative Gas Used Over Time
 - Average Confirmation Time per Block
 - Transactions per Block
 - Gas Used Over Time
