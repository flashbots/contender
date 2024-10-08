import os
from absl import app
from absl import flags
from absl import logging
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns

# Define command-line flags
flags.DEFINE_string('csv_path', None, 'Path to the folder containing the CSV files.')
flags.DEFINE_string('output_dir', None, 'Path to the directory where the plots will be saved.')
flags.DEFINE_integer('block_time', 2, 'Block time in seconds.')
FLAGS = flags.FLAGS

def read_csv_file(file_path):
    # Iterate over all files in the folder
    df =  pd.read_csv(file_path)
    return df


def print_stats(df):
    """
    Calculates and prints Ethereum transaction metrics:
    - Total number of transactions
    - Total gas used
    - Total time period in seconds
    - Transactions per second (TPS)
    - Gas used per second
    - Median time to inclusion (in milliseconds)
    - Mean time to inclusion (in milliseconds)

    Args:
        df (pd.DataFrame): DataFrame containing Ethereum transaction data.
    """
    # Check if necessary columns exist
    required_columns = {'start_time', 'end_time', 'gas_used', 'tx_hash'}
    if not required_columns.issubset(df.columns):
        missing = required_columns - set(df.columns)
        logging.error(f"Missing required columns: {missing}")
        return

    # Data Preprocessing
    try:
        # Convert 'start_time' and 'end_time' from milliseconds to datetime
        df['start_time'] = pd.to_datetime(df['start_time'], unit='ms')
        df['end_time'] = pd.to_datetime(df['end_time'], unit='ms')
    except Exception as e:
        logging.error(f"Error converting time columns: {e}")
        return

    # Calculate 'time_to_include' in milliseconds
    df['time_to_include'] = (df['end_time'] - df['start_time']).dt.total_seconds() * 1000  # in ms
    df['time_to_include_in_blocks'] = df['time_to_include'] / (FLAGS.block_time * 1000)

    # Calculate Total Number of Transactions
    total_transactions = len(df)

    # Calculate Total Gas Used
    if 'gas_used' in df.columns:
        total_gas = df['gas_used'].sum()
    else:
        logging.error("Column 'gas_used' not found.")
        return

    # Determine Total Time Period in Seconds
    start_time = df['start_time'].min()
    end_time = df['end_time'].max()
    total_time_seconds = (end_time - start_time).total_seconds()

    logging.info(f"Start Time: {start_time}")
    logging.info(f"End Time: {end_time}")
    logging.info(f"Total Time Period: {total_time_seconds:.2f} seconds")

    if total_time_seconds <= 0:
        logging.error("Total time period is zero or negative. Check 'start_time' and 'end_time' values.")
        return

    # Calculate Transactions Per Second (TPS) and Gas Used Per Second
    tx_per_second = total_transactions / total_time_seconds
    gas_per_second = total_gas / total_time_seconds

    # Calculate the Median and Mean of Time to Inclusion (in milliseconds)
    median_time_to_inclusion = df['time_to_include'].median()
    mean_time_to_inclusion = df['time_to_include'].mean()

    median_time_to_inclusion_in_blocks = df['time_to_include_in_blocks'].median()
    mean_time_to_inclusion_in_blocks = df['time_to_include_in_blocks'].mean()
    max_time_to_inclusion_in_blocks = df['time_to_include_in_blocks'].max()

    # Print the Results
    stats_output = (
        "\n===== Ethereum Transaction Metrics =====\n"
        f"Total Number of Transactions: {total_transactions}\n"
        f"Total Gas Used: {total_gas}\n"
        f"Total Time Period: {total_time_seconds:.2f} seconds\n"
        f"Transactions Per Second (TPS): {tx_per_second:.2f} tx/s\n"
        f"Gas Used Per Second: {gas_per_second:.2f} gas/s\n"
        f"Median Time to Inclusion: {median_time_to_inclusion:.2f} ms\n"
        f"Mean Time to Inclusion: {mean_time_to_inclusion:.2f} ms\n"
        f"Median Time to Inclusion (in blocks): {median_time_to_inclusion_in_blocks:.2f} blocks\n"
        f"Mean Time to Inclusion (in blocks): {mean_time_to_inclusion_in_blocks:.2f} blocks\n"
        f"Max Time to Inclusion (in blocks): {max_time_to_inclusion_in_blocks:.2f} blocks\n"
        "=========================================\n"
    )
    return stats_output


def plot_data(df, plots_info):
    """
    Generates and saves various plots related to Ethereum transactions.

    Args:
        df (pd.DataFrame): DataFrame containing Ethereum transaction data.
        plots_info (list): List to store tuples of (plot_filename, plot_title).
    """
    # Sample 0.5% of the DataFrame for plotting if needed
    sampled_df = df.sample(frac=0.005, random_state=42)

    plot_counter = 1

    # 1. Histogram of Transaction Confirmation Times (Time to Include)
    plt.figure(figsize=(10, 6))
    plt.hist(df['time_to_include'], bins=20, edgecolor='k')
    plot_title = 'Histogram of Transaction Confirmation Times'
    plt.title(plot_title)
    plt.xlabel('Time to Include (ms)')
    plt.ylabel('Frequency')
    # Save the plot
    plot_filename = f'plot_{plot_counter}_histogram_confirmation_times.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    plt.figure(figsize=(10, 6))
    plt.hist(df['time_to_include_in_blocks'], bins=20, edgecolor='k')
    plot_title = 'Histogram of Transaction Confirmation Times (in blocks)'
    plt.title(plot_title)
    plt.xlabel('Time to Include (blocks)')
    plt.ylabel('Frequency')
    # Save the plot
    plot_filename = f'plot_{plot_counter}_histogram_confirmation_blocks.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1


    # 2. Time Series of Confirmation Times (Sampled Data)
    plt.figure(figsize=(10, 6))
    plt.scatter(df['start_time'], df['time_to_include'], marker='o', alpha=0.6)
    plot_title = 'Time Series of Confirmation Times'
    plt.title(plot_title)
    plt.xlabel('Start Time')
    plt.ylabel('Time to Include (ms)')
    plt.xticks(rotation=45)
    plt.tight_layout()
    # Save the plot
    plot_filename = f'plot_{plot_counter}_time_series_confirmation_times.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    plt.figure(figsize=(10, 6))
    plt.scatter(df['start_time'], df['time_to_include_in_blocks'], marker='o', alpha=0.6)
    plot_title = 'Time Series of Confirmation Times in blocks'
    plt.title(plot_title)
    plt.xlabel('Start Time')
    plt.ylabel('Time to Include (blocks)')
    plt.xticks(rotation=45)
    plt.tight_layout()
    # Save the plot
    plot_filename = f'plot_{plot_counter}_time_series_confirmation_times_in_blocks.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # 3. Box Plot of Confirmation Times by Transaction Type
    plt.figure(figsize=(10, 6))
    sns.boxplot(x='kind', y='time_to_include', data=df)
    plot_title = 'Confirmation Times by Transaction Type'
    plt.title(plot_title)
    plt.xlabel('Transaction Type')
    plt.ylabel('Time to Include (ms)')
    plt.xticks(rotation=45)
    # Save the plot
    plot_filename = f'plot_{plot_counter}_boxplot_confirmation_times.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    plt.figure(figsize=(10, 6))
    sns.boxplot(x='kind', y='time_to_include_in_blocks', data=df)
    plot_title = 'Confirmation Times by Transaction Type'
    plt.title(plot_title)
    plt.xlabel('Transaction Type')
    plt.ylabel('Time to Include (blocks)')
    plt.xticks(rotation=45)
    # Save the plot
    plot_filename = f'plot_{plot_counter}_boxplot_confirmation_times_in_blocks.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # 4. Histogram of Gas Used
    plt.figure(figsize=(10, 6))
    plt.hist(df['gas_used'], bins=20, edgecolor='k')
    plot_title = 'Histogram of Gas Used'
    plt.title(plot_title)
    plt.xlabel('Gas Used')
    plt.ylabel('Frequency')
    # Save the plot
    plot_filename = f'plot_{plot_counter}_histogram_gas_used.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # 5. Bar Chart of Average Gas Used per Transaction Type
    avg_gas = df.groupby('kind')['gas_used'].mean().reset_index()
    plt.figure(figsize=(10, 6))
    sns.barplot(x='kind', y='gas_used', data=avg_gas)
    plot_title = 'Average Gas Used per Transaction Type'
    plt.title(plot_title)
    plt.xlabel('Transaction Type')
    plt.ylabel('Average Gas Used')
    plt.xticks(rotation=45)
    # Save the plot
    plot_filename = f'plot_{plot_counter}_bar_chart_avg_gas_used.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # 6. Scatter Plot of Gas Used vs. Confirmation Time
    plt.figure(figsize=(10, 6))
    plt.scatter(df['gas_used'], df['time_to_include'], alpha=0.7)
    plot_title = 'Gas Used vs. Confirmation Time'
    plt.title(plot_title)
    plt.xlabel('Gas Used')
    plt.ylabel('Time to Include (ms)')
    # Save the plot
    plot_filename = f'plot_{plot_counter}_scatter_gas_vs_confirmation_time.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # 7. Transaction Count Over Time with x-axis in seconds from start time
    # Ensure the data is sorted by 'start_time'
    df_sorted = df.sort_values('start_time')

    # Calculate 'seconds_from_start' for each timestamp
    df_sorted['seconds_from_start'] = (df_sorted['start_time'] - df_sorted['start_time'].min()).dt.total_seconds()

    # Set 'start_time' as the index for resampling
    df_sorted.set_index('start_time', inplace=True)

    # Resample the data per second to get transaction counts
    transaction_counts = df_sorted['tx_hash'].resample('1S').count()

    # Create a new DataFrame for plotting
    plot_df = transaction_counts.reset_index()

    # Calculate 'seconds_from_start' for the resampled data
    plot_df['seconds_from_start'] = (plot_df['start_time'] - plot_df['start_time'].min()).dt.total_seconds()

    # Plot the transaction counts over time with x-axis as seconds from start time
    plt.figure(figsize=(10, 6))
    plt.plot(plot_df['seconds_from_start'], plot_df['tx_hash'], marker='o', linestyle='-', alpha=0.7)
    plot_title = 'Transaction Count Over Time'
    plt.title(plot_title)
    plt.xlabel('Seconds from Start Time')
    plt.ylabel('Number of Transactions')
    plt.grid(True)
    plt.tight_layout()
    # Save the plot
    plot_filename = f'plot_{plot_counter}_transaction_count_over_time.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # Reset index and drop 'seconds_from_start' if no longer needed
    df_sorted.reset_index(inplace=True)
    df_sorted.drop('seconds_from_start', axis=1, inplace=True)

    # 8. Pie Chart of Transaction Types
    type_counts = df['kind'].value_counts()
    plt.figure(figsize=(8, 8))
    plt.pie(type_counts, labels=type_counts.index, autopct='%1.1f%%', startangle=140)
    plot_title = 'Distribution of Transaction Types'
    plt.title(plot_title)
    plt.axis('equal')
    # Save the plot
    plot_filename = f'plot_{plot_counter}_pie_chart_transaction_types.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # 10. Cumulative Gas Used Over Time
    df_cumulative = df.sort_values('end_time').copy()
    df_cumulative['cumulative_gas'] = df_cumulative['gas_used'].cumsum()
    plt.figure(figsize=(10, 6))
    plt.plot(df_cumulative['end_time'], df_cumulative['cumulative_gas'], linestyle='-', color='blue')
    plot_title = 'Cumulative Gas Used Over Time'
    plt.title(plot_title)
    plt.xlabel('End Time')
    plt.ylabel('Cumulative Gas Used')
    plt.xticks(rotation=45)
    plt.tight_layout()
    # Save the plot
    plot_filename = f'plot_{plot_counter}_cumulative_gas_over_time.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # 11. Average Confirmation Time per Block
    avg_time_per_block = df.groupby('block_number')['time_to_include'].mean().reset_index()
    plt.figure(figsize=(10, 6))
    plt.plot(avg_time_per_block['block_number'], avg_time_per_block['time_to_include'], marker='o', linestyle='-', color='green')
    plot_title = 'Average Confirmation Time per Block'
    plt.title(plot_title)
    plt.xlabel('Block Number')
    plt.ylabel('Average Time to Include (ms)')
    plt.xticks(rotation=45)
    plt.tight_layout()
    # Save the plot
    plot_filename = f'plot_{plot_counter}_avg_confirmation_time_per_block.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # 12. Distribution of Transactions per Block
    transactions_per_block = df['block_number'].value_counts().reset_index()
    transactions_per_block.columns = ['block_number', 'transaction_count']
    plt.figure(figsize=(10, 6))
    plt.bar(transactions_per_block['block_number'], transactions_per_block['transaction_count'], color='orange')
    plot_title = 'Transactions per Block'
    plt.title(plot_title)
    plt.xlabel('Block Number')
    plt.ylabel('Number of Transactions')
    plt.tight_layout()
    # Save the plot
    plot_filename = f'plot_{plot_counter}_transactions_per_block.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1

    # 13. Line Chart of Gas Used Over Time
    plt.figure(figsize=(10, 6))
    plt.plot(df_cumulative['end_time'], df_cumulative['gas_used'], marker='o', linestyle='-', color='red')
    plot_title = 'Gas Used Over Time'
    plt.title(plot_title)
    plt.xlabel('End Time')
    plt.ylabel('Gas Used')
    plt.xticks(rotation=45)
    plt.tight_layout()
    # Save the plot
    plot_filename = f'plot_{plot_counter}_gas_used_over_time.png'
    plt.savefig(os.path.join(FLAGS.output_dir, plot_filename), dpi=300)
    plt.close()
    plots_info.append((plot_filename, plot_title))
    plot_counter += 1


def main(_):
    # Ensure the plots directory exists
    if not os.path.exists(FLAGS.output_dir):
        os.makedirs(FLAGS.output_dir)
        logging.info(f"Created plots directory at '{FLAGS.output_dir}'.")

    # Read and combine CSV files into a DataFrame
    try:
        df = read_csv_file(FLAGS.csv_path)
    except ValueError as ve:
        logging.error(ve)
        return

    print_stats_output = print_stats(df)
    if not print_stats_output:
        logging.error("Error occurred while calculating Ethereum transaction metrics.")
        return

    logging.info(print_stats_output)

    # Generate and save plots, and collect plot information
    plots_info = []
    plot_data(df, plots_info)

    markdown_filename = os.path.join(FLAGS.output_dir, 'report.md')
    with open(markdown_filename, 'w') as md_file:
        md_file.write('# Conduit chain performance report\n\n')
        md_file.write(print_stats_output)
        for filename, title in plots_info:
            md_file.write(f'## {title}\n\n')
            md_file.write(f'![{title}]({filename})\n\n')

    logging.info(f"Plots have been saved to the '{FLAGS.output_dir}' directory.")
    logging.info(f"Markdown file '{markdown_filename}' has been generated.")


if __name__ == '__main__':
    flags.mark_flags_as_required(['csv_path', 'output_dir'])
    app.run(main)
